#![feature(plugin)]
#![feature(custom_derive)]
#![plugin(rocket_codegen)]

#[macro_use]
extern crate lazy_static;
extern crate regex;
extern crate rocket;

use regex::Regex;
use rocket::Outcome;
use rocket::http::Status;
use rocket::request;
use rocket::request::{Form, FromRequest, Request};
use rocket::response::Failure;

use std::thread;
use std::sync::{Arc, Mutex, RwLock, mpsc};

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

lazy_static! {
    static ref STAR_WORKER_CHANNEL: (Mutex<mpsc::Sender<StarRequestData>>, Mutex<mpsc::Receiver<StarRequestData>>) = {
        let (tx, rx) = mpsc::channel();
        // TODO: The mutex lock on this seems like a really wasteful overhead.
        (Mutex::new(tx), Mutex::new(rx))
    };
}

///
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
fn star(slack_form: Form<SlackSlashData>, worker_channel: WorkerChannel) -> Result<(), Status> {
    let slack_data = slack_form.into_inner();
    lazy_static! {
        // Sample url:
        // https://spychat.slack.com/archives/general/p1482786363038760
        static ref ARCHIVE_LINK_RE: Regex =
            Regex::new(r"https://\w+\.slack\.com/archives/\w+/p(?P<s>\d{10})(?P<us>\d{6})").unwrap();
    }

    let message_timestamp = match ARCHIVE_LINK_RE.captures(&slack_data.text) {
        Some(caps) => {
            format!("{}.{}", caps.name("s").unwrap(), caps.name("us").unwrap())
        },
        None => {
            return Err(Status::new(400, "Bad Request: Usage /quoth <link to message>"))
        },
    };
    let star_request_data = StarRequestData {
        user_id: slack_data.user_id,
        message_timestamp: message_timestamp,
        channel_id: slack_data.channel_id,
        response_url: slack_data.response_url,
    };
    return Ok(());
}

fn main() {
    rocket::ignite().mount("/", routes![star]).launch();
}
