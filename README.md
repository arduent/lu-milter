# lu‑milter

`lu‑milter` is a lightweight **Milter** written in Rust (built on [indymilter](https://crates.io/crates/indymilter)) that automatically generates a standards‑compliant \`List-Unsubscribe\` header for outbound messages.  Mail clients such as Gmail detect this header and render a one‑click *Unsubscribe* link.  The header contains a URL with an HMAC‑SHA‑256 token so you can safely verify unsubscribe requests server‑side.

---

## Features

- **Secure HMAC token** verifies that unsubscribe requests are genuine.
- Uses `MAIL FROM` and `RCPT TO` envelope addresses (no need to parse message bodies).
- Simple **TOML** configuration.
- Runs as a *Postfix* / *Sendmail* Milter over TCP.
- Drops root privileges after binding.
- Includes a sample **PHP** endpoint that validates the token and records unsubscribes.

---

## Configuration

Edit \`config.toml\` (example below) and adjust the fields:

```toml
listen_host = "127.0.0.1"      # where the milter listens
listen_port = 3000              # TCP port
url         = "https://example.com" # do not include trailing slash
stump       = "/rx/sample.php/" # path *after* base URL (include trailing slash)
delim       = "/-/-/"            # delimiter between address & hash (avoid plain "/")
secret      = "secretkittens"     # shared HMAC key
drop_user   = "lumilter"         # user the daemon drops to
```

> **Tip:** `stump` must end with `/`, while `url` **must not**.

---

## Building

```bash
# Requires Rust toolchain (https://rustup.rs)
$ cargo build --release
```

The resulting binary is at `target/release/lu-milter`.

---

## Running

```bash
$ sudo ./lu-milter \
      -p /run/lu-milter.pid \
      -l /var/lumilter/lu-milter.log \
      -c /var/lumilter/config.toml \
      -d
```

| Option                | Description                                  |
| --------------------- | -------------------------------------------- |
| `-c, --config <FILE>` | Path to TOML config                          |
| `-p, --pid <FILE>`    | Write PID file                               |
| `-d, --daemon`        | Run as background daemon                     |
| `-l, --log <FILE>`    | Log file (default: `/var/log/lu-milter.log`) |
| `-h, --help`          | Show help                                    |
| `-V, --version`       | Show version                                 |

---

## Postfix integration

Add to \`main.cf\`:

```conf
milter_protocol = 6
milter_default_action = accept
smtpd_milters   = inet:127.0.0.1:3000
non_smtpd_milters = $smtpd_milters
```

If you also sign with DKIM, list the DKIM milter **after** `lu-milter` so the added header doesn’t invalidate the signature.

---

## Sample PHP endpoint

`sample.php` demonstrates how to:

1. URL‑decode the `from`, `to`, and `hash` parameters.
2. Re‑compute the HMAC with the shared `secret`.
3. Save the pair to a **TAB‑separated** `.rems.tab` file.

Configure the same values you used in `config.toml`:

```php
$secret = "secretkittens";
$stump  = "/rx/sample.php/";
$delim  = "/-/-/";
$dotfile = ".rems.tab";
```

**Nginx security note** – prevent access to dot‑files:

```nginx
location ~ /\.
{
    deny all;
    access_log off;
    log_not_found off;
}
```

---

## License
```
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
```

