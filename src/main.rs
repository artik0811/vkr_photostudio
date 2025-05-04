use std::{collections::HashMap, sync::Arc};

use db::get_db_pool;
use handlers::{handle_callback_query, handle_message, UserSession};
use tokio::sync::Mutex;
mod models;
mod db;
mod handlers;
use teloxide::{
    dispatching::UpdateFilterExt,
    prelude::*,
    types::{CallbackQuery, InlineKeyboardMarkup, MaybeInaccessibleMessage},
};

extern crate pretty_env_logger;
#[macro_use] extern crate log;

#[tokio::main]
async fn main() {
    pretty_env_logger::init();
    let pool = get_db_pool().await;
    let bot = Bot::from_env();

    let bot = bot.clone();
    let pool = pool.clone();
    let user_sessions = Arc::new(Mutex::new(HashMap::<i64, UserSession>::new()));

    let handler = dptree::entry() 
    .branch(
        Update::filter_message().endpoint({
        let bot = bot.clone();
        let pool = pool.clone();
        let user_sessions = user_sessions.clone();

        move |bot, msg| {
            let pool = pool.clone();
            let user_sessions = user_sessions.clone();

            async move {
                let mut sessions = user_sessions.lock().await;
                handle_message(bot, msg, pool, &mut sessions).await;
                respond(())
            }
        }
    }))
    .branch(Update::filter_callback_query().endpoint({
        let bot = bot.clone();
        let pool = pool.clone();
        let user_sessions = user_sessions.clone();
    
        move |q: CallbackQuery, bot: Bot| {
            let pool = pool.clone();
            let user_sessions = user_sessions.clone();
    
            async move {
                let mut sessions = user_sessions.lock().await;
                let query = q.clone();
                if let Some(message) = MaybeInaccessibleMessage::regular_message(&query.message.unwrap()) {
                    handle_callback_query(q, bot, message.clone(), pool, &mut sessions).await;
                }
                respond(())
            }
        }
    }));

    Dispatcher::builder(bot, handler)
        .enable_ctrlc_handler()
        .build()
        .dispatch()
        .await;
}