use log::info;

use mongodb::bson::{doc, DateTime};
use mongodb::options::ReturnDocument;
use mongodb::Database;

use crate::models::online::{Online, OnlineDocument};

pub async fn insert_online(
    db: &Database,
    uuid: &str,
    api_token: &str,
    online: bool,
) -> mongodb::error::Result<Option<Online>> {
    info!(target: "app", "insert_online - Called");
    let collection = db.collection::<OnlineDocument>("online");

    let online_doc = collection
        .find_one_and_update(
            doc! { "uuid": uuid, "apiToken": api_token },
            doc! {
                "$set": {
                    "online": online,
                    "modifiedAt": DateTime::now(),
                },
                "$setOnInsert": {
                    "createdAt": DateTime::now(),
                }
            },
        )
        .upsert(true)
        .return_document(ReturnDocument::After)
        // TODO ATTENTION I should check and return a custom DbError here Err(....) and not unwrap with "?" and ignore the error.
        .await?;

    // return result
    match online_doc {
        Some(online_doc) => Ok(Some(document_to_json(&online_doc))),
        None => {
            log::error!(target: "app", "insert_online - Cannot find and update online");
            // TODO ATTENTION I should return a custom DbError here Err(....) and not Ok.
            Ok(None)
        }
    }
}

fn document_to_json(online_doc: &OnlineDocument) -> Online {
    Online {
        _id: online_doc.id.to_string(),
        uuid: online_doc.uuid.to_string(),
        apiToken: online_doc.apiToken.to_string(),
        createdAt: online_doc.createdAt.to_string(),
        modifiedAt: online_doc.modifiedAt.to_string(),
        online: online_doc.online,
    }
}
