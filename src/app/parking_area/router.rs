use std::path::Path as StdPath;

use axum::{
    extract::{Path, State},
    middleware,
    routing::{get, patch, post},
    Router,
};
use chrono::{NaiveDateTime, Utc};
use serde::{Deserialize, Serialize};
use sqlx::{PgPool, Pool, Postgres};
use tracing::debug;
use uuid::Uuid;

use crate::{
    app::user::{Role, User},
    error::aggregate::{Error, Result},
    extractor::{app_body::Body, app_json::AppSuccess},
    middleware::base::print_request_body,
};

use super::{ParkingLot, ParkingLotWithCountOfKeeper, UpdateParkingLot};

pub fn build(pool: Pool<Postgres>) -> Router {
    let router = Router::new()
        .route("/", post(create))
        .route("/:id", patch(update).get(detail))
        .route("/owner/:id", get(get_by_owner))
        .layer(middleware::from_fn(print_request_body))
        .with_state(pool);

    Router::new().nest("/parking-lot", router)
}

#[derive(Serialize, Deserialize)]
struct CreateParkingLotPayload {
    area_name: String,
    address: String,
    file_name: Option<String>,
    car_cost: f64,
    motor_cost: f64,
    owner_id: Uuid,
}

impl CreateParkingLotPayload {
    fn into_parking_lot(self) -> ParkingLot {
        ParkingLot {
            id: Uuid::new_v4(),
            area_name: self.area_name,
            address: self.address,
            image_url: "some url".to_string(),
            car_cost: self.car_cost,
            motor_cost: self.motor_cost,
            owner_id: self.owner_id,
            created_at: Some(Utc::now().naive_utc()),
            updated_at: None,
        }
    }
}

async fn create(
    State(pool): State<PgPool>,
    Body(payload): Body<CreateParkingLotPayload>,
) -> Result<AppSuccess<ParkingLot>> {
    let user = User::find_one_by_id(payload.owner_id, &pool).await?;
    if user.role != Role::ParkOwner {
        return Err(Error::BadRequest(
            "Related user is not having owner role".to_string(),
        ));
    }

    // let dir = format!("./public/files/{}", payload.file_name);
    // let path = StdPath::new(&dir);
    // if !path.exists() {
    //     return Err(Error::BadRequest("Image not found".to_string()));
    // }

    let parking_lot = payload.into_parking_lot();
    let parking_lot = parking_lot.save(&pool).await?;
    Ok(AppSuccess(parking_lot))
}

#[derive(Serialize, Deserialize)]
struct UpdateParkingLotPayload {
    area_name: Option<String>,
    address: Option<String>,
    file_name: Option<String>,
    car_cost: Option<f64>,
    motor_cost: Option<f64>,
    owner_id: Option<Uuid>,
    park_keeper_ids: Option<Vec<Uuid>>,
}

impl UpdateParkingLotPayload {
    fn into_update_parking_lot(self) -> UpdateParkingLot {
        UpdateParkingLot {
            id: None,
            area_name: self.area_name,
            address: self.address,
            image_url: self.file_name,
            car_cost: self.car_cost,
            motor_cost: self.motor_cost,
            owner_id: self.owner_id,
            created_at: None,
            updated_at: Some(Utc::now().naive_utc()),
        }
    }
}

async fn update(
    State(pool): State<PgPool>,
    Path(id): Path<Uuid>,
    Body(payload): Body<UpdateParkingLotPayload>,
) -> Result<AppSuccess<ParkingLot>> {
    match &payload.owner_id {
        Some(owner_id) => {
            let user = User::find_one_by_id(*owner_id, &pool).await?;
            if user.role != Role::ParkOwner {
                return Err(Error::BadRequest(
                    "Related user is not having owner role".to_string(),
                ));
            }
        }
        None => {}
    }

    match &payload.file_name {
        Some(file_name) => {
            let dir = format!("./public/files/{}", *file_name);
            let path = StdPath::new(&dir);
            if !path.exists() {
                return Err(Error::BadRequest("Image not found".to_string()));
            }
        }
        None => {}
    }

    match &payload.park_keeper_ids {
        Some(ids) => {
            if ids.len() > 0 {
                User::remove_parking_lot(id, &pool).await?;
                User::update_parking_lot(id, ids.clone(), &pool).await?;
            }
        }
        None => {}
    }

    let parking_lot = payload.into_update_parking_lot();
    let parking_lot = parking_lot.update(id, &pool).await?;
    Ok(AppSuccess(parking_lot))
}

#[derive(Serialize, Deserialize)]
pub struct DetailParkingLot {
    pub id: Option<Uuid>,
    pub area_name: Option<String>,
    pub address: Option<String>,
    pub image_url: Option<String>,
    pub car_cost: Option<f64>,
    pub motor_cost: Option<f64>,
    pub owner_id: Option<Uuid>,
    pub created_at: Option<NaiveDateTime>,
    pub updated_at: Option<NaiveDateTime>,
    pub keepers: Option<Vec<KeeperOnDetailParkingLot>>,
}

#[derive(Serialize, Deserialize)]
pub struct KeeperOnDetailParkingLot {
    pub id: Option<Uuid>,
    pub name: Option<String>,
}

async fn detail(
    State(pool): State<PgPool>,
    Path(id): Path<Uuid>,
) -> Result<AppSuccess<DetailParkingLot>> {
    let parking_lot = ParkingLot::detail(id, &pool).await?;

    let keeper_info: Vec<KeeperOnDetailParkingLot> = parking_lot
        .clone()
        .into_iter()
        .filter_map(|item| {
            if let (Some(id), Some(name)) = (item.keeper_id, item.keeper_name) {
                Some(KeeperOnDetailParkingLot {
                    id: Some(id),
                    name: Some(name),
                })
            } else {
                None
            }
        })
        .collect();

    let mut detail = DetailParkingLot {
        id: None,
        area_name: None,
        address: None,
        image_url: None,
        car_cost: None,
        motor_cost: None,
        owner_id: None,
        created_at: None,
        updated_at: None,
        keepers: Some(Vec::new()), // Initialize keepers as an empty Vec
    };

    for data in parking_lot {
        detail = DetailParkingLot {
            id: Some(data.id),
            area_name: Some(data.area_name),
            address: Some(data.address),
            image_url: Some(data.image_url),
            car_cost: Some(data.car_cost),
            motor_cost: Some(data.motor_cost),
            owner_id: Some(data.owner_id),
            created_at: data.created_at,
            updated_at: data.updated_at,
            keepers: Some(Vec::new()), // Initialize keepers as an empty Vec
        };
    }

    // Set the `keepers` field to `keeper_info` if it's not empty
    if !keeper_info.is_empty() {
        detail.keepers = Some(keeper_info);
    }

    // Now `detail.keepers` will be an empty Vec if no valid keepers exist, or it will be set to the valid keepers

    Ok(AppSuccess(detail))
}

async fn get_by_owner(
    State(pool): State<PgPool>,
    Path(owner_id): Path<Uuid>,
) -> Result<AppSuccess<Vec<ParkingLotWithCountOfKeeper>>> {
    let parking_lot = ParkingLot::find_by_owner(owner_id, &pool).await?;
    Ok(AppSuccess(parking_lot))
}
