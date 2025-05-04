use sqlx::postgres::PgPoolOptions;
use dotenvy::dotenv;
use std::env;

pub async fn get_db_pool() -> sqlx::Pool<sqlx::Postgres> {
    dotenv().ok();
    let database_url = env::var("DATABASE_URL").expect("DATABASE_URL not set");
    PgPoolOptions::new()
        .connect(&database_url)
        .await
        .expect("Failed to connect to DB")
}