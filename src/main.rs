#[allow(non_snake_case)]
#[cfg(test)]
mod tests;
mod api;
mod database;

#[macro_use]
extern crate rocket;

use mongodb::bson::doc;
use mongodb::error::Error;
use rocket::futures::TryStreamExt;
use rocket::response::Redirect;
use serde_json::Value;
use crate::api::v1::mount_v1;
use crate::database::{connect, Domain};

const DOMAIN: &str = "https://lmpk.tk";
const DATABASE_NAME: &str = "redirector";
// collection for domains in debug
#[cfg(debug_assertions)]
const DOMAINS_COLLECTION: &str = "domainsDev";
// collection for domains in release
#[cfg(not(debug_assertions))]
const DOMAINS_COLLECTION: &str = "domains";
// collection for auth codes
const AUTH_COLLECTION: &str = "auth";

#[get("/<name>")]
async fn redirector(name: String) -> Redirect {
    let col = connect().await.collection::<Domain>(DOMAINS_COLLECTION);
    let filter = doc! { "name" : name };
    let mut cursor = match col.find(filter, None).await {
        Ok(c) => c,
        Err(_) => return Redirect::to(DOMAIN)
    };
    let dom = match cursor.try_next().await {
        Ok(c) => c,
        Err(_) => return Redirect::to(DOMAIN)
    };
    match dom {
        Some(d) => Redirect::to(d.domain),
        None => return Redirect::to(DOMAIN)
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