use sqlx::FromRow;
use chrono::{DateTime, Utc};

#[derive(Debug, FromRow)]
pub struct Photographer {
    pub id: i32,
    pub name: String,
}

#[derive(Debug, FromRow)]
pub struct Client {
    pub id: i32,
    pub telegram_id: i64,
    pub name: String,
}

#[derive(Debug, FromRow)]
pub struct Booking {
    pub id: i32,
    pub client_id: i32,
    pub photographer_id: i32,
    pub service: String,
    pub booking_start: DateTime<Utc>,
    pub booking_end: DateTime<Utc>,
    pub status: String,
}

#[derive(Debug, FromRow)]
pub struct Material {
    pub id: i32,
    pub booking_id: i32,
    pub file_url: String,
}

#[derive(Debug, FromRow)]
pub struct Service {
    pub id: i32,
    pub name: String,
    pub cost: i32,
    pub duration: i32,
}

#[derive(Debug, sqlx::FromRow)]
pub struct PhotographerService {
    pub id: i32,
    pub photographer_id: i32,
    pub service_id: i32,
}
