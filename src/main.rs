/*

    Copyright (C) 2025 Waitman Gobble

    This program is free software; you can redistribute it and/or modify
    it under the terms of the GNU General Public License as published by
    the Free Software Foundation; either version 2 of the License, or
    (at your option) any later version.

    This program is distributed in the hope that it will be useful,
    but WITHOUT ANY WARRANTY; without even the implied warranty of
    MERCHANTABILITY or FITNESS FOR A PARTICULAR PURPOSE.  See the
    GNU General Public License for more details.

    You should have received a copy of the GNU General Public License along
    with this program; if not, see <https://www.gnu.org/licenses/>.

   Contact by email: <waitman@quantificant.com>
   <https://quantificant.com/contact>

*/

use clap::{Arg, Command};
use nix::unistd::{setgid, setuid, Gid, Uid};
use byte_strings::c_str;
use indymilter::{
    Actions, Callbacks, Context, EomContext, NegotiateContext, ProtoOpts, 
    Status, ContextActions, SocketInfo,
};
use futures::future::pending;
use serde::Deserialize;
use std::{ffi::CString, process, fs, fs::File};
use daemonize::Daemonize;
use time::macros::format_description;
use simplelog::*;
use percent_encoding::{utf8_percent_encode, NON_ALPHANUMERIC};
use hmac::{Hmac, Mac};
use rsa::sha2::Sha256;
use base64::engine::general_purpose::STANDARD as BASE64;
use base64::engine::Engine as _;
use rand::distributions::{Alphanumeric, DistString};

type HmacSha256 = Hmac<Sha256>;

// context data
struct ElmData {
    rcpt_to: Option<String>,
    mail_from: Option<String>,
}

// Server configuration structure
#[derive(Debug, Deserialize)]
struct Config {
    listen_host: String,
    listen_port: u16,
    drop_user: String,
    secret: String,
    url: String,
    stump: String,
    delim: String,
}

// software version
fn get_version() -> &'static str {
  "0.1a"
}

// logging macro
macro_rules! log_expect {
    ($result:expr, $msg:expr) => {
         match $result {
             Ok(val) => val,
             Err(err) => {
                 log::error!("{}: {}", $msg, err);
                 panic!("{}: {}", $msg, err);
             }
         }
    };
}

// drop from root to unprivileged user specified in config
fn drop_privileges(unprivileged_user: &str) -> Result<(), Box<dyn std::error::Error>> {
    let user = users::get_user_by_name(unprivileged_user)
        .ok_or_else(|| format!("User {} not found", unprivileged_user))?;
    setgid(Gid::from_raw(user.primary_group_id()))?;
    setuid(Uid::from_raw(user.uid()))?;
    Ok(())
}

// create hash verification token
fn generate_hash_token(mail_from: &str, rcpt_to: &str, secret: &str) -> String {
    let mut mac = HmacSha256::new_from_slice(secret.as_bytes())
        .expect("hmac nokey nomac");
    mac.update(mail_from.as_bytes());
    mac.update(rcpt_to.as_bytes());

    let result = mac.finalize();
    to_base64(result.into_bytes().to_vec())
}

// base64 encoder helper function
fn to_base64(text: Vec<u8>) -> String {
        BASE64.encode(text)
}

// initialize file logger
fn init_file_logger(log_file: &str) {
    let file = File::options()
        .append(true)
        .create(true)
        .open(log_file)
        .expect("Failed to create/open log file");

    // Create a custom format closure if you want to include timestamps.
    let config = ConfigBuilder::new()
        .set_time_format_custom(format_description!("[year]-[month]-[day] [hour]:[minute]:[second]"))
        .build();

    WriteLogger::init(LevelFilter::Info, config, file)
        .expect("Failed to initialize file logger");
}

fn main() {

    // version set at the top of the file
    let version  = get_version();

    // parse command-line arguments
    let matches = Command::new("LU Milter")
        .version(version)
        .author("Waitman Gobble <waitman@quantificant.com>")
        .about("A simple milter that adds List-Unsubscribe headers to emails.")
        .arg(
            Arg::new("config")
                .short('c')
                .long("config")
                .value_name("FILE")
                .help("Sets a custom config file")
                .num_args(1),
        )
        .arg(
            Arg::new("pid")
                .short('p')
                .long("pid")
                .value_name("FILE")
                .help("PID file path")
                .num_args(1),
        )
        .arg(
            Arg::new("daemon")
                .short('d')
                .long("daemon")
                .help("Run as a daemon")
                .action(clap::ArgAction::SetTrue),
        )
        .arg(
            Arg::new("log")
                .short('l')
                .long("log")
                .value_name("FILE")
                .help("Sets the log file path")
                .default_value("/var/log/lu-milter.log")
                .num_args(1),
        )
        .get_matches();

    // get config file path; default to "config.toml" if not specified
    let config_path = matches.get_one::<String>("config").map(String::as_str).unwrap_or("config.toml");

    let pid_file = matches.get_one::<String>("pid").map(String::as_str);
    let run_daemon = *matches.get_one::<bool>("daemon").unwrap_or(&false);
    let log_file = matches.get_one::<String>("log").unwrap();

    // start logging
    init_file_logger(log_file);

    // log our PID
    let pid = process::id();
    log::info!("+++ lu-milter version {:?} started with PID {}",version,pid);
    log::info!("Using configuration file: {}", config_path.to_string());
    log::info!("Logging to {}",log_file);

    if run_daemon {
        let stdout = File::options()
            .append(true)
            .create(true)
            .open(log_file)
            .expect("Failed to create/open log file");
        let stderr = stdout.try_clone().expect("Failed to clone log file handle");

        let mut daemonize = Daemonize::new()
            .stdout(stdout)
            .stderr(stderr);

        // create PID file if specified
        // can use with rc script
        if let Some(pid_path) = pid_file {
            daemonize = daemonize.pid_file(pid_path);
        }

        match daemonize.start() {
            Ok(_) => log::info!("Daemonized"),
            Err(e) => {
                // oops we gotta bail
                log::error!("Error during daemonization: {}", e);
                process::exit(1);
            }
        }
    }

    let rt = tokio::runtime::Builder::new_multi_thread()
        .enable_all()
        .build()
        .unwrap();

    rt.block_on(async_main(config_path.to_string()));

}

async fn async_main(config_path:String) {

    // load the configuration
    let config_content = log_expect!(fs::read_to_string(config_path.clone()),
        "Failed to read configuration file {config_path:?}");
    let config: Config = log_expect!(toml::from_str(&config_content),
        "Failed to parse configuration");

    log::info!("Loaded Config {}",config_path);

    let address = format!("{}:{}", config.listen_host, config.listen_port);
    let listener = match tokio::net::TcpListener::bind(&address).await {
        Ok(l) => l,
        Err(e) => {
            eprintln!("Failed to bind socket: {}", e);
            return;
        }
    };
    log::info!("lu milter is listening on {}", address);

    //drop to unprivileged user
    if !config.drop_user.is_empty() {
        let _ = drop_privileges(&config.drop_user);
        log::info!("Dropped to user: {}",config.drop_user);
    }

    //get shared secret for hmac

    let hmac_secret = if !config.secret.is_empty() {
        log::info!("Using shared HMAC secret from config.");
        config.secret
    } else {
        let generated = Alphanumeric.sample_string(&mut rand::thread_rng(), 16);
        log::info!("Shared HMAC secret not specified. Generated: {}", generated);
        generated
    };

    let url = format!("{}{}",
        config.url,
        config.stump);
    let delim = config.delim;

    let callbacks = Callbacks::new()
        .on_negotiate(|context, actions, opts| Box::pin(handle_negotiate(context, actions, opts)))
        .on_connect(|context, hostname, socket_info| Box::pin(handle_connect(context, hostname, socket_info)))
        .on_helo(|context, hostname| Box::pin(handle_helo(context, hostname)))
        .on_mail(|context, args| Box::pin(handle_mail(context, args)))
        .on_rcpt(|context, args| Box::pin(handle_rcpt(context, args)))
        .on_eom(move |context| Box::pin(handle_eom(hmac_secret.clone(),
            url.clone(),delim.clone(),context)))
        .on_abort(|context| Box::pin(handle_abort(context)))
        .on_close(|context| Box::pin(handle_close(context)))
        .on_unknown(|context, arg| Box::pin(handle_unknown(context, arg)));

    let milter_config = Default::default();

    indymilter::run(listener, callbacks, milter_config, pending::<()>())
        .await
        .expect("milter execution failed");
}

async fn handle_negotiate(
    context: &mut NegotiateContext<ElmData>,
    _: Actions,
    _: ProtoOpts,
) -> Status {
    log::info!("NEGOTIATE");
    context.requested_actions |= Actions::ADD_HEADER;

    Status::AllOpts
}

async fn handle_connect(
    context: &mut Context<ElmData>,
    hostname: CString,
    socket_info: SocketInfo,
) -> Status {
    log::info!("CONNECT");
    log::info!("  hostname: {hostname:?}");
    log::info!("  socket_info: {socket_info:?}");

    let elm_data = ElmData {
        mail_from: None,
        rcpt_to: None,
    };

    // store context for laters
    context.data = Some(elm_data);

    Status::Continue
}

async fn handle_helo(_context: &mut Context<ElmData>, hostname: CString) -> Status {
    log::info!("HELO {hostname:?}");

    Status::Continue
}

async fn handle_mail(context: &mut Context<ElmData>, _args: Vec<CString>) -> Status {

    if let Some(elm_data) = &mut context.data {
        if let Some(mail_from) = context.macros.get(c_str!("{mail_addr}")) {
            let mail_from = mail_from.to_string_lossy();
            elm_data.mail_from = Some(mail_from.clone().into());
            log::info!("MAIL FROM: {mail_from:?}");
        } else {
            log::error!("MAIL FROM: FAILED");
        }
    } else {
        log::error!("MAIL FROM: NO CONTEXT");
    }

    Status::Continue
}

async fn handle_rcpt(context: &mut Context<ElmData>, _args: Vec<CString>) -> Status {

    if let Some(elm_data) = &mut context.data {
        if let Some(rcpt_to) = context.macros.get(c_str!("{rcpt_addr}")) {
            let rcpt_to = rcpt_to.to_string_lossy();
            elm_data.rcpt_to = Some(rcpt_to.clone().into());
            log::info!("RCPT TO: {rcpt_to:?}");
        } else {
            log::error!("RCPT TO: FAILED");
        }
    } else {
        log::error!("RCPT TO: NO CONTEXT");
    }

    Status::Continue
}

async fn handle_eom(
    secret:String, 
    url:String, 
    delim:String,
    context: &mut EomContext<ElmData>) -> Status {

    log::info!("EOM");

    if let Some(ElmData { mail_from, rcpt_to }) = context.data.take() {
        let mail_from = mail_from.unwrap_or_else(|| "none".to_owned());
	let rcpt_to = rcpt_to.unwrap_or_else(|| "none".to_owned());


	let b64_hash = generate_hash_token(&mail_from, &rcpt_to, &secret);
	let b64_from = to_base64(mail_from.into_bytes().to_vec());
	let b64_to = to_base64(rcpt_to.into_bytes().to_vec());

        let encoded_hash = utf8_percent_encode(&b64_hash, NON_ALPHANUMERIC).to_string();
        let encoded_from = utf8_percent_encode(&b64_from, NON_ALPHANUMERIC).to_string();
        let encoded_rcpt = utf8_percent_encode(&b64_to, NON_ALPHANUMERIC).to_string();

        let unsubscribe_url = format!("<{}{}{}{}{}{}>",
            &url, encoded_rcpt, &delim, encoded_from, &delim, encoded_hash
        );

        log::info!("Unsubscribe URL: {}",unsubscribe_url);

        context.actions.add_header("List-Unsubscribe-Post", "List-Unsubscribe=One-Click").await.unwrap();
        context.actions.add_header("List-Unsubscribe", unsubscribe_url).await.unwrap();
    } else {
        log::error!("EOM failed, no Context ElmData");
    }

    Status::Continue
}

async fn handle_abort(_: &mut Context<ElmData>) -> Status {
    log::info!("ABORT****");

    Status::Continue
}

async fn handle_close(_: &mut Context<ElmData>) -> Status {
    log::info!("CLOSE");

    Status::Continue
}

async fn handle_unknown(_: &mut Context<ElmData>, arg: CString) -> Status {
    log::info!("UNKNOWN");
    log::info!("  arg: {arg:?}");

    Status::Continue
}

