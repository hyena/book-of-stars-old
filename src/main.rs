#![feature(plugin)]
#![feature(custom_derive)]
#![plugin(rocket_codegen)]

extern crate rocket;
extern crate rocket_codegen;

use rocket::request::{Form, FromForm};

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

#[post("/starlord", data = "<data>")]
fn star(data: Form<SlackSlashData>) -> Result<&'static str, String> {
    println!("{:?}", data);
    return Ok("Penned into the Book of Stars.");
}

fn main() {
    rocket::ignite().mount("/", routes![star]).launch();
}
