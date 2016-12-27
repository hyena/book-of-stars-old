#![feature(plugin)]
#![feature(custom_derive)]
#![plugin(rocket_codegen)]

extern crate hyper;
#[macro_use]
extern crate lazy_static;
extern crate regex;
extern crate rocket;
extern crate rustc_serialize;
extern crate slack_api;
extern crate toml;

use hyper::Client;
use hyper::header::ContentType;
use regex::Regex;
use rocket::Outcome;
use rocket::http::Status;
use rocket::request;
use rocket::request::{Form, FromRequest, Request};
use rocket::response::status;
use rustc_serialize::json;
use slack_api::Error;
use slack_api::Message;
use slack_api::channels::history;
use slack_api::stars::add;

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

/// Data that we will JSON encode to send back in response to a message.
#[derive(Debug, RustcEncodable)]
struct SlashCommandResponse<'a> {
    response_type: &'a str,
    text: &'a str,
    // TODO(hyena): Support attachments for fancier responses.
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
                                  "Bad verification token."));
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
    let client = Client::new();
    let child = thread::Builder::new().name("staring thread".to_string()).spawn(move || {
        fn send_slack_response(client: &Client, url: &str, text: &str) {
            let body = json::encode(&SlashCommandResponse {
                response_type: "ephemeral",
                text: &text,
            }).unwrap();
            match client.post(url).header(ContentType::json()).body(&body).send() {
                Ok(res) => println!("Sent response successfully."),  // TODO(hyena): More checking.
                Err(e) => println!("Error sending response! {}", e),
            };
        }

        let rx = STAR_WORKER_CHANNEL.1.lock().unwrap();
        loop {
            let star_request_data = rx.recv().unwrap();
            println!("Got this timestamp {}", star_request_data.message_timestamp);
            let msg: Message;
            match history(&client, &CONFIG.slack_token, &star_request_data.channel_id,
                          Some(&star_request_data.message_timestamp),
                          Some(&star_request_data.message_timestamp),
                          Some(true),
                          Some(1))
            {
                Ok(mut history_response) => {
                    if history_response.messages.len() != 1 {
                        send_slack_response(&client, &star_request_data.response_url,
                                            "Couldn't retrieve that message.");
                        continue;
                    }
                    msg = history_response.messages.remove(0);
                }
                Err(e) => {
                    send_slack_response(&client, &star_request_data.response_url,
                                        "Couldn't retrieve that message.");
                    continue;// TODO: Return an error response here.
                }
            };

            println!("Message: {:?}", msg);
            match msg {
                Message::Standard {text: Some(ref text), .. } => {
                    let res_text = match add(&client, &CONFIG.slack_token, None, None,
                                             Some(&star_request_data.channel_id),
                                             Some(&star_request_data.message_timestamp))
                    {
                        Ok(_) => format!("Penned \"{}\" into the book of stars.... 🐼", &text),
                        // Already being starred is okay.
                        Err(Error::Api(ref s)) if s == "already_starred" =>
                            format!("Penned \"{}\" into the book of stars.... 🐼", &text),
                        _ => format!(concat!("Alack! Could not pen \"{}\" into the book of stars....",
                                      "\nBother perhaps the foolish sqrl? 🐿"), &text),
                    };
                    send_slack_response(&client, &star_request_data.response_url, &res_text);
                },
                _ => send_slack_response(&client, &star_request_data.response_url,
                                         "Unexpected message."),
            }
        }
    });
    rocket::ignite().mount("/", routes![star]).launch();
}
