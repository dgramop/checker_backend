use std::{env, error::Error};

use reqwest::Client;
use schema::{members, taken, workshops};

extern crate reqwest;
extern crate tokio;
#[macro_use]
extern crate serde;
extern crate serde_json;
#[macro_use]
extern crate rocket;
extern crate diesel;
#[macro_use]
extern crate derive_more;
use diesel::result::Error as DBError;

extern crate tl;

use rocket::{form::Form, serde::json::Json, State};
use tl::{Node, NodeHandle, ParserOptions};
pub mod schema;

use diesel::{prelude::*, result::DatabaseErrorKind};
use tokio::sync::RwLock;

const ALUMNUS: [u32; 1] = [1254375];

fn establish_connection() -> SqliteConnection {
    //PgConnection::establish("postgres://127.0.0.1/checker").expect("Should be able to connect to database")
    SqliteConnection::establish("sqlite.db").expect("Should be able to connect to database")
}

async fn get_client() -> Result<Client, reqwest::Error> {
    let reqw = reqwest::ClientBuilder::new()
        .cookie_store(true)
        .build()
        .expect("builds reqwest client");

    //TODO: Clean up
    let login_body = reqw
        .post("https://awrgmu.atriumcampus.com/activity/mix/do_login")
        .form(&[
            (
                "username",
                env::var("ATRIUM_USERNAME").expect("ATRIUM_USERNAME env variable must be set"),
            ),
            (
                "password",
                env::var("ATRIUM_PASSWORD").expect("ATRIUM_PASSWORD env variable must be set"),
            ),
        ])
        .send()
        .await?
        .text()
        .await?;

    println!("Logged in... {}", login_body);

    Ok(reqw)
}

#[derive(Deserialize, Debug, Serialize)]
pub struct CheckInEligibility {
    code: String,
    eligible: bool,
}

#[derive(Deserialize, Debug, Serialize)]
#[serde(untagged)]
pub enum CheckInAtrium {
    Detailed {
        success: bool,
        html: String,
        eligibility: CheckInEligibility,
    },
    Undetailed {
        success: bool,
        message: String,
    },
}

#[derive(Debug, Serialize)]
#[serde(tag = "entry")]
pub enum CheckInResp {
    Allow {
        /// HTML passed directly from the API
        #[serde(skip)]
        html: String,

        /// Name of the student, if were able to parse it
        name: String,

        /// GNumber of the student, if we were able to parse it
        gnum: u32,

        /// Workshops the student has completed
        workshops: Vec<(Taken, Workshop)>,
    },
    Disallow {
        /// HTML response from the Atrium backend, usually containing an error message
        html: String,
    },
}

enum CheckInError {
    AtriumParserError,
    DBError,
}

#[derive(Queryable, Selectable, Insertable, Debug, Serialize)]
#[diesel(table_name = schema::members)]
pub struct Member {
    gnum: i32,
    is_staff: bool,
}

#[derive(Queryable, Selectable, Insertable, Debug, Serialize)]
#[diesel(table_name = schema::taken)]
pub struct Taken {
    id: String,
    member: i32,
    workshop: String,
}

#[derive(Queryable, Selectable, Insertable, Debug, Serialize)]
#[diesel(table_name = schema::workshops)]
pub struct Workshop {
    id: String,
    name: String,
}

#[derive(Display, Debug, Serialize)]
pub enum TakeWorkshopError {
    AlreadyTook,
    DBError,
}

impl Error for TakeWorkshopError {}

// should support card ID or gnumber?
/// Records the fact a student took a workshop
#[post("/api/members/<gnum>/workshop/<workshop>")]
async fn take_workshop(
    gnum: u32,
    workshop: uuid::Uuid,
) -> Result<Json<(Taken, Workshop)>, Json<TakeWorkshopError>> {
    let mut conn = establish_connection();

    let _member = match members::dsl::members
        .filter(members::dsl::gnum.eq(gnum as i32))
        .get_result::<Member>(&mut conn)
    {
        Ok(member) => member,
        _ => return Err(Json(TakeWorkshopError::DBError)),
    };

    let workshop = match workshops::dsl::workshops
        .filter(workshops::dsl::id.eq(workshop.to_string()))
        .get_result::<Workshop>(&mut conn)
    {
        Ok(workshop) => workshop,
        _ => return Err(Json(TakeWorkshopError::DBError)),
    };

    let inserted_taken_id = uuid::Uuid::new_v4().to_string();

    match diesel::insert_into(taken::dsl::taken)
        .values(Taken {
            id: inserted_taken_id.clone(),
            member: gnum as i32,
            workshop: workshop.id.clone(),
        })
        .execute(&mut conn)
    {
        Ok(taken) => taken,
        Err(DBError::DatabaseError(DatabaseErrorKind::UniqueViolation, _)) => {
            return Err(Json(TakeWorkshopError::AlreadyTook))
        }
        Err(_) => return Err(Json(TakeWorkshopError::DBError)),
    };

    match taken::dsl::taken
        .filter(taken::dsl::id.eq(inserted_taken_id))
        .get_result::<Taken>(&mut conn)
    {
        Ok(taken) => Ok(Json((taken, workshop))),
        Err(_) => return Err(Json(TakeWorkshopError::DBError)),
    }
}

/// Records the fact a student took a workshop
#[delete("/api/members/<gnum>/workshop/<workshop>")]
async fn untake_workshop(
    gnum: u32,
    workshop: uuid::Uuid,
) -> Result<Json<(Taken, Workshop)>, Json<TakeWorkshopError>> {
    let mut conn = establish_connection();

    let _member = match members::dsl::members
        .filter(members::dsl::gnum.eq(gnum as i32))
        .get_result::<Member>(&mut conn)
    {
        Ok(member) => member,
        _ => return Err(Json(TakeWorkshopError::DBError)),
    };

    let workshop = match workshops::dsl::workshops
        .filter(workshops::dsl::id.eq(workshop.to_string()))
        .get_result::<Workshop>(&mut conn)
    {
        Ok(workshop) => workshop,
        _ => return Err(Json(TakeWorkshopError::DBError)),
    };

    let to_delete = taken::dsl::taken
        .filter(taken::dsl::member.eq(gnum as i32))
        .filter(taken::dsl::workshop.eq(&workshop.id))
        .clone();

    // split up for sqlite support since we can't do RETURNING
    let deleted_taken = match to_delete.get_result::<Taken>(&mut conn) {
        Ok(taken) => taken,
        Err(_) => return Err(Json(TakeWorkshopError::DBError)),
    };

    match diesel::delete(to_delete).execute(&mut conn) {
        Ok(_) => Ok(Json((deleted_taken, workshop))),
        Err(DBError::DatabaseError(DatabaseErrorKind::UniqueViolation, _)) => {
            return Err(Json(TakeWorkshopError::AlreadyTook))
        }
        Err(_) => return Err(Json(TakeWorkshopError::DBError)),
    }
}

/// lists workshops so that a staff member can select a particular one to check students in for
#[get("/api/workshops")]
async fn list_workshops() -> Result<Json<Vec<Workshop>>, ()> {
    let mut conn = establish_connection();
    match workshops::dsl::workshops.get_results::<Workshop>(&mut conn) {
        Ok(workshops) => Ok(Json(workshops)),
        Err(e) => {
            eprintln!("Error when loading workshops {:?}", e);
            Err(())
        }
    }
}

/// tries to shallow delete a workshop
#[delete("/api/workshops/<workshop>")]
async fn delete_workshop(workshop: uuid::Uuid) -> Result<(), ()> {
    let mut conn = establish_connection();

    match diesel::delete(
        workshops::dsl::workshops.filter(workshops::dsl::id.eq(workshop.to_string())),
    )
    .execute(&mut conn)
    {
        Ok(_) => Ok(()),
        Err(e) => {
            eprintln!("Error when loading workshops {:?}", e);
            Err(())
        }
    }
}

#[derive(FromForm)]
pub struct CreateWorkshopForm {
    name: String,
}

/// Creates a new workshop
#[post("/api/workshops", data = "<workshop_form>")]
async fn add_workshop(workshop_form: Form<CreateWorkshopForm>) -> Result<(), ()> {
    let mut conn = establish_connection();
    match diesel::insert_into(workshops::dsl::workshops)
        .values(Workshop {
            id: uuid::Uuid::new_v4().to_string(),
            name: workshop_form.into_inner().name,
        })
        .execute(&mut conn)
    {
        Ok(_) => Ok(()),
        Err(e) => {
            eprintln!("Error when loading workshops {:?}", e);
            Err(())
        }
    }
}

/// Checks in a customer, returns gnumber from card/netid, and if the customer should be allowed
/// into the mix or not
#[post("/api/check_in/<id>")]
async fn check_in(id: String, state: &State<St>) -> Json<CheckInResp> {
    //Json<CheckInResponse> {

    //let reqw = get_client().await.unwrap();

    let atrium_req = state
        .client
        .read()
        .await
        .client
        .post("https://awrgmu.atriumcampus.com/activity/mix/ajax/basic_search")
        .form(&[("card_number", id)]);

    let mut atrium = atrium_req
        .try_clone()
        .expect("atrium request should be cloneable")
        .send()
        .await
        .unwrap()
        .json::<CheckInAtrium>()
        .await
        .unwrap();

    // Check if the server threw an error because we weren't logged in, in which case log-in and retry the
    // request
    match &atrium {
        CheckInAtrium::Undetailed { success, message }
            if *success == false && message == "log_out" =>
        {
            state.client.write().await.client =
                get_client().await.expect("should log-in to atrium");
            eprintln!("Was logged out of atrium, logging in and trying again");
            atrium = atrium_req
                .send()
                .await
                .unwrap()
                .json::<CheckInAtrium>()
                .await
                .unwrap();
        }
        _ => (),
    }

    match atrium {
        CheckInAtrium::Detailed {
            success: _,
            html,
            eligibility,
        } => {
            // Since the HTML could change and parsing it too tightly could lead to premature
            // failure, to make this system more futureproof, we pull certain data out of the HTML
            // on a best-effort basis. May ditch this since if we can't parse the HTML for a
            // g-number, you might as well ditch the system and go back to using atrium
            //

            let mut conn = establish_connection();

            let (name, gnum) = {
                let parsed_html = tl::parse(&html, ParserOptions::default())
                    .expect("HTML from API should be parsable");
                let parser = parsed_html.parser();

                let person_name: String = match parsed_html.get_element_by_id("person_name") {
                    Some(name) => name.get(parser).unwrap().inner_text(parser).to_string(),
                    None => {
                        panic!()
                    }
                };

                let gnum: u32 = match parsed_html.get_elements_by_class_name("campus_id").next() {
                    Some(id) => {
                        let mut found_id: Option<u32> = None;
                        for child in NodeHandle::get(&id, parser)
                            .unwrap()
                            .children()
                            .unwrap()
                            .top()
                            .iter()
                        {
                            if let Node::Raw(b) = NodeHandle::get(child, parser).unwrap() {
                                let candidate_id = b.as_utf8_str().to_string();
                                eprintln!("{}", candidate_id);

                                if let Ok(parsed_integer_id) =
                                    u32::from_str_radix(&candidate_id.trim(), 10)
                                {
                                    found_id = Some(parsed_integer_id)
                                }
                            }
                        }

                        found_id.expect("Must have an ID in html")
                    }
                    None => {
                        panic!()
                    }
                };

                (person_name, gnum)
            };

            // TODO: try to insert member into members table
            let _ = diesel::insert_into(schema::members::table)
                .values(Member {
                    gnum: gnum as i32,
                    is_staff: false,
                })
                .execute(&mut conn);

            // TODO: Load the member's workshops,
            let workshops: Vec<(Taken, Workshop)> = match taken::dsl::taken
                .filter(taken::dsl::member.eq(gnum as i32))
                .inner_join(
                    workshops::dsl::workshops.on(workshops::dsl::id.eq(taken::dsl::workshop)),
                )
                .get_results::<(Taken, Workshop)>(&mut conn)
            {
                Ok(workshops) => workshops,
                Err(e) => {
                    eprintln!(
                        "Non-fatal error loading workshops for gnum {} with error: {:?}",
                        gnum, e
                    );
                    Vec::new()
                }
            };

            // If the person is eligible normally, or if they were rejected since they already
            // swiped in in the last minute, allow the person in. In this failure mode, we still
            // get gnumber and name information
            if eligibility.eligible || eligibility.code == "DENY902" {
                return Json(CheckInResp::Allow {
                    html,
                    name,
                    gnum,
                    workshops,
                });
            } else if ALUMNUS.contains(&gnum) {
                return Json(CheckInResp::Allow {
                    html,
                    name,
                    gnum,
                    workshops,
                });
            } else {
                return Json(CheckInResp::Disallow { html });
            }
        }
        CheckInAtrium::Undetailed {
            success: _,
            message,
        } => return Json(CheckInResp::Disallow { html: message }),
    }
}

struct AtriumClient {
    client: Client,
}

struct St {
    client: RwLock<AtriumClient>,
}

#[launch]
async fn rocket() -> _ {
    rocket::build()
        .manage(St {
            client: RwLock::new(AtriumClient {
                client: get_client().await.expect("client logs in"),
            }),
        })
        .mount(
            "/",
            routes![
                check_in,
                take_workshop,
                list_workshops,
                add_workshop,
                untake_workshop,
                delete_workshop
            ],
        )
}
