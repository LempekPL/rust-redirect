use std::fmt::{Display, Formatter};
use std::{env, process};
use mongodb::{Client, Database};
use mongodb::bson::Bson;
use mongodb::bson::oid::ObjectId;
use mongodb::error::ErrorKind;
use mongodb::options::ClientOptions;
use rocket::Config;
use rocket::tokio::join;
use serde::{Serialize, Deserialize};
use crate::{add_and, AUTH_COLLECTION, DATABASE_NAME, DOMAINS_COLLECTION};

#[derive(Deserialize, Serialize, Debug, Clone)]
pub(crate) struct Domain {
    pub(crate) _id: ObjectId,
    pub(crate) name: String,
    pub(crate) domain: String,
    pub(crate) owner: ObjectId,
}

#[derive(Deserialize, Serialize, Debug)]
pub(crate) struct Auth {
    pub(crate) _id: ObjectId,
    pub(crate) name: String,
    pub(crate) password: String,
    pub(crate) permission: Permission,
}

#[derive(Deserialize, Clone)]
struct MoConfig {
    db_host: String,
    db_port: u16,
    db_user: String,
    db_password: String,
}

impl Default for MoConfig {
    fn default() -> Self {
        Self {
            db_host: "localhost".to_string(),
            db_port: 27017,
            db_user: "admin".to_string(),
            db_password: "".to_string(),
        }
    }
}

pub(crate) async fn connect() -> Database {
    let conf = if env::var("CI").unwrap_or("false".to_string()).parse::<bool>().unwrap_or(false) {
        MoConfig {
            db_host: "localhost".to_string(),
            db_port: 27017,
            db_user: "admin".to_string(),
            db_password: "pass".to_string()
        }
    } else {
        match Config::figment().extract::<MoConfig>() {
            Ok(conf) => conf,
            Err(_) => {
                println!("Database config not found. Using default values");
                MoConfig::default()
            }
        }
    };
    let client = re_conn(conf, 3).await;
    client.database(DATABASE_NAME)
}

#[async_recursion::async_recursion]
async fn re_conn(config: MoConfig, tries: u8) -> Client {
    match connect_to_database(config.clone()).await {
        Ok(c) => c,
        Err(e) => {
            if tries == 0 {
                println!("Could not connect to the database: {:?} \n\x1b[31mTerminating process\x1b[0m", *e.kind);
                process::exit(1);
            }
            println!("Could not connect to the database: {:?} \x1b[34m(Remaining tries: {})\x1b[0m", *e.kind, tries);
            re_conn(config, tries - 1).await
        }
    }
}

async fn connect_to_database(config: MoConfig) -> mongodb::error::Result<Client> {
    // TODO: ability to use url
    let mut client_options = ClientOptions::parse(
        format!("mongodb://{}:{}@{}:{}/",
                config.db_user, config.db_password, config.db_host, config.db_port)
    ).await?;
    client_options.app_name = Some("RustRedirect".to_string());
    let client = Client::with_options(client_options)?;
    Ok(client)
}

#[async_recursion::async_recursion]
async fn create_collection_unless(db: &Database, name: &str, tries: u8) {
    match db.create_collection(name, None).await {
        Ok(_) => println!("Created collection {}", name),
        Err(e) => {
            match *e.kind {
                ErrorKind::Command(c) if c.code == 48 => {
                    println!("Collection '{}' already exists", name)
                }
                _ => {
                    if tries == 0 {
                        println!("Could not create collection: {:?} \n\x1b[31mTerminating process\x1b[0m", *e.kind);
                        process::exit(1);
                    }
                    println!("Could not create collection: {:?} \x1b[34m(Remaining tries: {})\x1b[0m", *e.kind, tries);
                    create_collection_unless(db, name, tries - 1).await;
                }
            }
        }
    }
}

pub(crate) async fn manage_database() {
    let db = connect().await;

    let create_domains = create_collection_unless(&db, DOMAINS_COLLECTION, 3);
    let create_auths = create_collection_unless(&db, AUTH_COLLECTION, 3);
    join!(create_domains, create_auths);

    // add default auth if not found any
    let a_col = db.collection::<Auth>(AUTH_COLLECTION);
    if let Ok(count) = a_col.count_documents(None, None).await {
        if count == 0 {
            let h = match bcrypt::hash("pass", bcrypt::DEFAULT_COST) {
                Ok(k) => k,
                Err(e) => panic!("Could not hash. {:?}", e)
            };
            let res = a_col.insert_one(Auth {
                _id: Default::default(),
                name: "admin".to_string(),
                password: h,
                permission: Permission(1, 0, 0, 0, 0, 0),
            }, None).await;
            match res {
                Ok(_) => println!("No auth found, created new auth"),
                Err(e) => panic!("Could not create default user. {:?}", e)
            }
        }
    }
}

// Permission(0, 0, 0, 0, 0)
// 0 - full admin
// 1 - add/remove/edit auths lower than this and list all auths except admin
// 2 - edit/delete all redirects
// 3 - list all redirects
// 4 - create/edit/delete/list own redirects
// 5 - create random named redirects

#[derive(Deserialize, Serialize, Debug, Clone, Copy)]
pub(crate) struct Permission(u8, u8, u8, u8, u8, u8);

impl Permission {
    // can do anything they want
    pub(crate) fn can_admin(&self) -> bool {
        self.0 == 1
    }

    // can add/remove/edit auths lower than themself and list all auths except admin
    pub(crate) fn can_manage(&self) -> bool {
        self.1 == 1 || self.can_admin()
    }

    // can edit/delete all redirects
    pub(crate) fn can_mod(&self) -> bool {
        self.2 == 1 || self.can_admin()
    }

    // can list all redirects
    pub(crate) fn can_list(&self) -> bool {
        self.3 == 1 || self.can_admin()
    }

    // can create/edit/delete/list own redirects
    pub(crate) fn can_own(&self) -> bool {
        self.4 == 1 || self.can_admin()
    }

    // can create random named redirects
    pub(crate) fn can_random(&self) -> bool {
        self.5 == 1 || self.can_admin()
    }

    // can nothing
    pub(crate) fn can_nothing(&self) -> bool {
        self.0 == 0 && self.1 == 0 && self.2 == 0 && self.3 == 0 && self.4 == 0 && self.5 == 0
    }

    pub(crate) fn to_vec(self) -> Vec<u8> {
        vec![self.0, self.1, self.2, self.3, self.4, self.5]
    }

    pub(crate) fn from_vec(nums: Vec<u8>) -> Permission {
        Permission(
            *nums.get(0).unwrap_or(&0),
            *nums.get(1).unwrap_or(&0),
            *nums.get(2).unwrap_or(&0),
            *nums.get(3).unwrap_or(&0),
            *nums.get(4).unwrap_or(&0),
            *nums.get(5).unwrap_or(&0),
        )
    }

    pub(crate) fn from_u8(num: u8) -> Permission {
        let bin = format!("{:06b}", num).trim().to_owned();
        dbg!(&bin);
        let bins: Vec<u8> = bin
            .chars()
            .map(|x| x
                .to_string()
                .parse::<u8>()
                .unwrap_or(0)
            )
            .collect();
        Permission::from_vec(bins)
    }
}

impl From<Permission> for Bson {
    fn from(p: Permission) -> Self {
        let p = p
            .to_vec()
            .iter()
            .map(|p| Bson::Int32(*p as i32))
            .collect::<Vec<Bson>>();
        Bson::Array(p)
    }
}

impl Default for Permission {
    fn default() -> Self {
        Permission(0, 0, 0, 0, 0, 0)
    }
}

impl Display for Permission {
    fn fmt(&self, f: &mut Formatter<'_>) -> std::fmt::Result {
        if self.can_admin() {
            write!(f, "can admin (do anything they want)")
        } else {
            let mut str: String = "".to_string();
            if self.can_manage() {
                str += "can manage (add/remove/edit auths lower than themself and list all auths except admin)";
            }
            if self.can_mod() {
                add_and!(str);
                str += "can mod (edit/delete all redirects)";
            }
            if self.can_list() {
                add_and!(str);
                str += "can list (list all redirects)";
            }
            if self.can_own() {
                add_and!(str);
                str += "can own (create/edit/delete/list own redirects)";
            }
            if self.can_random() {
                add_and!(str);
                str += "can random (create random named redirects)";
            }
            if self.can_nothing() {
                str = "can nothing (no permissions to do anything)".to_string();
            }
            write!(f, "{}", str)
        }
    }
}