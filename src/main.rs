#![feature(plugin)]
#![feature(custom_derive)]
#![plugin(rocket_codegen)]

#[macro_use]
extern crate lazy_static;
extern crate regex;
extern crate rocket;
extern crate rustc_serialize;
extern crate slack_api;
extern crate toml;

use regex::Regex;
use rocket::Outcome;
use rocket::http::Status;
use rocket::request;
use rocket::request::{Form, FromRequest, Request};
use rocket::response::status;

use std::fs::File;
use std::io::Read;
use std::thread;
use std::sync::{Mutex, mpsc};

#[derive(Debug, FromForm)]
struct SlackSlashData {
    token: String,
    team_id: String,
    team_domain: String,
    channel_id: String,
    channel_name: String,
    user_id: String,
    user_name: String,
    command: String,
    text: String,
    response_url: String,
}

/// Data to be set to our background thread to record a star.
struct StarRequestData {
    user_id: String,
    message_timestamp: String,
    channel_id: String,
    response_url: String,
}

/// Represents the contents of config.toml
#[derive(Debug, RustcDecodable)]
struct Config {
    slack_token: String,
    verification_token: String,
}

/// Attempt to load and parse the config file into our Config struct.
/// If a file cannot be found, return a default Config.
/// If we find a file but cannot parse it, panic
fn parse_config() -> Config {
    let mut config_toml = String::new();

    let mut file = match File::open("config.toml") {
        Ok(file) => file,
        Err(_)  => {
            panic!("Could not find config file! Is 'config.toml' set up?");
        }
    };

    file.read_to_string(&mut config_toml)
            .unwrap_or_else(|err| panic!("Error while reading config.toml: [{}]", err));

    println!("{}", config_toml);
    match toml::decode_str(&config_toml) {
        Some(config) => config,
        None => panic!("Error while deserializing config.toml.")
    }
}

lazy_static! {
    static ref STAR_WORKER_CHANNEL: (Mutex<mpsc::Sender<StarRequestData>>, Mutex<mpsc::Receiver<StarRequestData>>) = {
        let (tx, rx) = mpsc::channel();
        // TODO: The mutex lock on this seems like a really wasteful overhead.
        (Mutex::new(tx), Mutex::new(rx))
    };
    //#[derive(Debug)]
    static ref CONFIG: Config = parse_config();
}

struct WorkerChannel {
    tx: mpsc::Sender<StarRequestData>,
}

impl<'a, 'r> FromRequest<'a, 'r> for WorkerChannel {
    type Error = ();

    fn from_request(request: &'a Request<'r>) -> request::Outcome<WorkerChannel, ()> {
        Outcome::Success(WorkerChannel {
            tx: STAR_WORKER_CHANNEL.0.lock().unwrap().clone(),
        })
    }
}

#[post("/starlord", data = "<slack_form>")]
fn star(slack_form: Form<SlackSlashData>, worker_channel: WorkerChannel) ->
    Result<(), status::Custom<&str>>
{
    let slack_data = slack_form.into_inner();
    lazy_static! {
        // Sample url:
        // https://spychat.slack.com/archives/general/p1482786363038760
        static ref ARCHIVE_LINK_RE: Regex =
            Regex::new(r"https://\w+\.slack\.com/archives/\w+/p(?P<s>\d{10})(?P<us>\d{6})")
            .unwrap();
    }
    if slack_data.token != CONFIG.verification_token {
        return Err(status::Custom(Status::Forbidden,
                                  "Bad token."));
    }
    let message_timestamp = match ARCHIVE_LINK_RE.captures(&slack_data.text) {
        Some(caps) => {
            format!("{}.{}", caps.name("s").unwrap(), caps.name("us").unwrap())
        },
        None => {
            return Err(status::Custom(Status::BadRequest,
                                      "Bad Request: Usage /quoth <link to message>"));
        },
    };
    let star_request_data = StarRequestData {
        user_id: slack_data.user_id,
        message_timestamp: message_timestamp,
        channel_id: slack_data.channel_id,
        response_url: slack_data.response_url,
    };
    worker_channel.tx.send(star_request_data).unwrap();
    Ok(())
}

fn main() {
    let child = thread::Builder::new().name("staring thread".to_string()).spawn(move || {
        let rx = STAR_WORKER_CHANNEL.1.lock().unwrap();
        loop {
            let star_request_data = rx.recv().unwrap();
            println!("Got this timestamp {}", star_request_data.message_timestamp);
        }
    });
    rocket::ignite().mount("/", routes![star]).launch();
}
