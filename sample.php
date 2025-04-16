<?php

/* Config */
$secret = "secretkittens";
$stump = "/rx/i.php/";
$delim = "/-/-/";
$dotfile = ".rems.tab";
$lf = "\n"; //line terminator


/* ** ** * ** ** * * ** *** * ** ** *
  Block dot files in your web server
  or put the removes in a database

  nginx:

    location ~ /\. {
        deny all;
        access_log off;
        log_not_found off;
    }
 * *** * * *** * * * * ** * ** * * */


/*DEBUG for 'get' testing*/

/*
$_POST=array();
$_POST['List-Unsubscribe']='One-Click';
*/

$ip = $_SERVER['REMOTE_ADDR'];

if (isset($_SERVER['DOCUMENT_URI']))
{
        if (isset($_POST['List-Unsubscribe']))
        {
                if ($_POST['List-Unsubscribe'] == 'One-Click')
                {
                        $uri = str_replace($stump,'',$_SERVER['DOCUMENT_URI']);
                        $jx=explode($delim,$uri);
                        $rx=array_pop($jx);
                        $lx=array_pop($jx);
                        $from = base64_decode($lx);
                        $lx=array_pop($jx);
                        $to = base64_decode($lx);
                        $uid = base64_decode($rx);
                        $hash = hash_hmac('sha256', $from.$to , $secret, true);

                        if (hash_equals($uid,$hash))
                        {
                                $fp=fopen($dotfile,'a');
                                fwrite($fp,time()."\t".$ip."\t".$from."\t".$to.$lf);
                                fclose($fp);
                                echo 'OK';
                        } else {
                                //DEBUG
                                //echo bin2hex($uid).' <:ne:> '.bin2hex($hash);
                        }
                }
        }
}

exit();


