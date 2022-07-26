mod api;
mod database;
#[cfg(test)]
mod tests;

#[macro_use]
extern crate rocket;

use mongodb::bson::doc;
use rocket::response::Redirect;
use crate::api::v1::mount_v1;
use crate::database::{connect, Domain};

const DOMAIN: &str = "https://lmpk.tk";
const DATABASE_NAME: &str = "redirector";
// collection for domains in debug
#[cfg(debug_assertions)]
const DOMAINS_COLLECTION: &str = "devDomains";
// collection for domains in release
#[cfg(not(debug_assertions))]
const DOMAINS_COLLECTION: &str = "domains";
// collection for auth codes in debug
#[cfg(debug_assertions)]
const AUTH_COLLECTION: &str = "devAuth";
// collection for auth codes in release
#[cfg(not(debug_assertions))]
const AUTH_COLLECTION: &str = "auth";

#[get("/<name>")]
async fn redirector(name: String) -> Redirect {
    let col = connect().await.collection::<Domain>(DOMAINS_COLLECTION);
    let filter = doc! { "name" : name };
    let dom = ok_return!(col.find_one(filter, None).await, Redirect::to(DOMAIN));
    match dom {
        Some(d) => Redirect::to(d.domain),
        None => Redirect::to(DOMAIN)
    }
}

#[get("/")]
fn index() -> &'static str {
    "Hello, world!"
}

#[rocket::main]
async fn main() -> Result<(), rocket::Error> {
    database::manage_database().await;
    // build, mount and launch
    let rocket = rocket::build()
        .mount("/", routes![index])
        // change `r` to change redirecting prefix e.g. example.com/r/<name of redirect>
        .mount("/r", routes![redirector]);
    let rocket = mount_v1(rocket);
    let _rocket = rocket.launch()
        .await?;

    Ok(())
}

macro_rules! some_return {
    ( $e:expr, $r:expr ) => {
        match $e {
            Some(x) => x,
            None => return $r,
        }
    }
}

macro_rules! ok_return {
    ( $e:expr, $r:expr ) => {
        match $e {
            Ok(x) => x,
            Err(e) => {
                println!("{:?}", e);
                return $r;
            },
        }
    }
}

macro_rules! add_and {
    ( $s:expr ) => {
        if !$s.is_empty() {
           $s = $s + " and "
        }
    }
}

pub(crate) use some_return;
pub(crate) use ok_return;
pub(crate) use add_and;
