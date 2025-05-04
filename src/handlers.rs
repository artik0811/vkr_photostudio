use chrono::{Duration, NaiveDate, NaiveDateTime, NaiveTime};
use log::{error};
use teloxide::{prelude::*, types::{CallbackQuery, InlineKeyboardButton, InlineKeyboardMarkup, KeyboardButton, KeyboardMarkup, Message, ReplyMarkup, User}, RequestError};
use sqlx::{postgres::PgQueryResult, query::{self, Map}, PgPool};
use core::slice;
use std::{collections::HashMap, vec};
use time::{macros::{format_description, time}, Date, Month, PrimitiveDateTime, Time};
use chrono::Datelike;

use crate::models::{Client, Photographer, Service};
extern crate pretty_env_logger;

// Структура для хранения сессии пользователя
pub struct UserSession {
    step: UserStep,
    client_id: i32,
    service_id: Option<i32>,
    photographer_id: Option<i32>,
    selected_date: Option<Date>,
    selected_time_start: Option<Time>,
    selected_time_end: Option<Time>,
    agreement: bool
}

impl UserSession {
    fn new() -> Self {
        UserSession {
            step: UserStep::Registartion,
            client_id: -1,
            service_id: None,
            photographer_id: None,
            selected_date: None,
            selected_time_start: None,
            selected_time_end: None,
            agreement: false
        }
    }
}

// Перечисление шагов процесса
#[derive(Debug, Clone, Copy)]
enum UserStep {
    Registartion,
    Start,
    MainMenu,
    HistoryOfBookings,

    SelectingService,
    SelectingPhotographer,
    SelectingTime,
    ConfirmingBooking,
    Payment,
}

//todo
/*
    1. Согласие на обработку данных, 
    2. возмонжость отозвать его и после отзыва хранить клиента в архиве
    3. Кнопку "Любой фотограф" после выбора услуги

*/

pub async fn handle_message(msg: Message, bot: Bot, pool: PgPool, user_sessions: &mut HashMap<i64, UserSession>) {
    let chat_id = msg.chat.id;
    let text = msg.text().unwrap_or_else(|| "");

    let session = user_sessions.entry(chat_id.0).or_insert(UserSession::new());
    if let Some(text) = msg.text() {
        match text {
            "/start" => {
                session.step = UserStep::Start;
            }
            "Выбрать услугу" => {
                if session.client_id == -1 {
                    session.step = UserStep::Start;
                } else {
                    session.step = UserStep::MainMenu;
                }
            },
            "История заказов" => {
                if session.client_id == -1 {
                    session.step = UserStep::Start;
                } else {
                    session.step = UserStep::MainMenu;
                }
            }
            _ => {}
        }
    }

    match session.step {
        UserStep::Registartion => {
            println!("Registartion: {}", text);
            let name = text.to_string();
            sqlx::query("INSERT INTO clients (telegram_id, name) VALUES ($1, $2) ON CONFLICT (telegram_id) DO NOTHING")
                .bind(chat_id.0 as i32)
                .bind(name)
                .execute(&pool)
                .await
                .unwrap();
            bot.send_message(chat_id, "Ты успешно зарегистрирован!")
                .await
                .unwrap();
            session.client_id = sqlx::query_scalar("SELECT id FROM clients WHERE telegram_id = $1")
                .bind(chat_id.0 as i32)
                .fetch_one(&pool)
                .await
                .unwrap();
            session.step = UserStep::MainMenu;
            let buttons: Vec<Vec<KeyboardButton>> = vec![
                vec![KeyboardButton::new("Выбрать услугу")],
                vec![KeyboardButton::new("История заказов")],
            ];

            let keyboard = KeyboardMarkup::new(buttons).resize_keyboard();
            bot.send_message(chat_id, "Выбери действие")
                .reply_markup(ReplyMarkup::Keyboard(keyboard))
                .await
                .unwrap();
        }

        UserStep::Start => {
            println!("Start : {}", text);
            let client = check_client(&pool, chat_id.0).await;
            let buttons: Vec<Vec<KeyboardButton>> = vec![
                vec![KeyboardButton::new("Выбрать услугу")],
                vec![KeyboardButton::new("История заказов")],
            ];

            let keyboard = KeyboardMarkup::new(buttons).resize_keyboard();
            if client.is_some() {
                bot.send_message(chat_id, "Привет! Я бот для бронирования фотографов. Как я могу помочь?")
                    .reply_markup(ReplyMarkup::Keyboard(keyboard))
                    .await
                    .unwrap();
                session.step = UserStep::MainMenu;
                session.client_id = client.unwrap().id;
            } else {
                session.step = UserStep::Registartion;
                bot.send_message(chat_id, "Привет! Я бот для бронирования фотографов. Пожалуйста, введи свое имя:")
                    .await
                    .unwrap();
                return;
            }
            session.step = UserStep::MainMenu;
        }

        UserStep::SelectingService => {
            println!("SelectingService: {}", text);
        }

        UserStep::MainMenu => {
            println!("Main Menu: {}", text);
            if !session.agreement {
                show_agreement(bot.clone(), chat_id).await;
                return;
            }
            if text == "Выбрать услугу" {
                session.step = UserStep::SelectingService;
                show_services(bot.clone(), chat_id, &pool).await;
            } else if text == "История заказов" {
                session.step = UserStep::HistoryOfBookings;
                bot.send_message(chat_id, "Здесь будет история заказов.")
                    .await
                    .unwrap();
            }
        }

        UserStep::SelectingPhotographer => {
            println!("SelectingPhotographer: {}", text);
        }

        UserStep::SelectingTime => {
            println!("Select time");
        }

        UserStep::ConfirmingBooking => {
            println!("ConfirmingBooking: {}", text);

        }

        UserStep::HistoryOfBookings => {
            println!("HistoryOfBookings: {}", text);

        }

        UserStep::Payment => {
            println!("Payment: {}", text);

        }
    }
}

pub async fn handle_callback_query(q: CallbackQuery, bot: Bot, msg: Message, pool: PgPool, user_sessions: &mut HashMap<i64, UserSession>) -> Result<(), RequestError> {
    println!("Callback query: {:?}", q.data.clone().unwrap());
    let chat_id: ChatId = msg.chat.id;
    let text: &str = msg.text().unwrap_or_else(|| "");
    let session: &mut UserSession = user_sessions.entry(chat_id.0).or_insert(UserSession::new());

    if let Some(data) = q.data.clone() {
        if data.starts_with("calendar:") {
            let parts: Vec<&str> = data.split(':').collect();
            println!("Data: {}", data);
            match parts.as_slice() {
                ["calendar", "next_month", current_month, current_year] => {
                    let mut month: u32 = current_month.parse().unwrap();
                    let mut year: i32 = current_year.parse().unwrap();
                    if month == 12 {
                        month = 1;
                        year += 1;
                    } else {
                        month += 1;
                    }
                    println!("month: {}, year: {}", month, year);
                    let new_calendar = generate_calendar(month, year);

                    if let Some(msg) = q.message.clone() {
                        bot.edit_message_reply_markup(msg.chat().id, msg.id())
                            .reply_markup(new_calendar)
                            .await?;
                    }
                }
                ["calendar", "prev_month", current_month, current_year] => {
                    let mut month: u32 = current_month.parse().unwrap();
                    let mut year: i32 = current_year.parse().unwrap();
                    if month == 1 {
                        month = 12;
                        year -= 1;
                    } else {
                        month -= 1;
                    }

                    let new_calendar = generate_calendar(month, year);

                    if let Some(msg) = q.message.clone() {
                        bot.edit_message_reply_markup(msg.chat().id, msg.id())
                            .reply_markup(new_calendar)
                            .await?;
                    }
                }
                ["calendar", "select", date] => {
                    println!("Selected date: {}", date);
                    let format = format_description!("[year]-[month]-[day] [hour]:[minute]:[second]");
                    let mut parsed_date = String::new();
                    parsed_date.push_str(date);

                    let date_format = format_description!("[year]-[month]-[day]");
                    session.selected_date = Date::parse(*date, &date_format).ok();

                    // если выбранная дата находится в прошлом
                    if validate_selected_date(date).await == false {
                        bot.send_message(q.from.id, "Выбери дату в будущем")
                            .await?;
                        let today_month = chrono::Utc::now().month();
                        let toady_year = chrono::Utc::now().year();
                        let key = generate_calendar(today_month, toady_year);
                        bot.send_message(chat_id, "Выбери дату:")
                                .reply_markup(ReplyMarkup::InlineKeyboard(key))
                                .await
                                .unwrap();
                        return Ok(());
                    }

                    parsed_date.push_str(" 00:00:00");
                    let free_slots = get_free_slots(
                        &pool,
                        session.photographer_id.unwrap(),
                        session.service_id.unwrap(),
                        PrimitiveDateTime::parse(&parsed_date, &format).unwrap()
                    ).await.unwrap();
                    bot.send_message(q.from.id, format!("Выбери свободное время: {}", date))
                        .reply_markup(ReplyMarkup::InlineKeyboard(
                            InlineKeyboardMarkup::new(free_slots.iter().map(|slot| {
                                vec![InlineKeyboardButton::callback(slot.clone(), format!("time-{}", slot))]
                            }))
                        ))
                        .await?;
                }
                _ => {}
            }
            bot.answer_callback_query(q.id).await?;
        } else if data.starts_with("time-") {
            println!("Time selected: {}", data);
            let teims = data.split("-").collect::<Vec<&str>>();
            let format = format_description!("[hour]:[minute]");
            match teims.as_slice() {
                ["time", start,end] => {
                    session.selected_time_start = Some(Time::parse(&start, &format).unwrap());
                    session.selected_time_end = Some(Time::parse(&end, &format).unwrap());

                    let time: String = format!("{}:{:02}-{}:{:02}", session.selected_time_start.unwrap().hour(), session.selected_time_start.unwrap().minute(), session.selected_time_end.unwrap().hour(), session.selected_time_end.unwrap().minute());
                    let service = sqlx::query_as::<_, Service>(
                        "SELECT * FROM services WHERE id = $1"
                    )
                    .bind(session.service_id.unwrap())
                    .fetch_one(&pool)
                    .await
                    .unwrap();

                    let photographer = sqlx::query_as::<_, Photographer>(
                        "SELECT * FROM photographers WHERE id = $1",
                    )
                    .bind(session.photographer_id.unwrap())
                    .fetch_one(&pool)
                    .await
                    .unwrap();

                    let confirm_button: Vec<String> = vec!["Подтвердить".to_string(), "Изменить".to_string()];
                    let confirm_action: Vec<String> = vec!["yes".to_string(), "no".to_string()];
                    let key:InlineKeyboardMarkup = generate_inline_markup("confirming", confirm_button, confirm_action);
                    //todo выбор филиала(optional), показывать адрес
                    let order_string = format!("Ваш заказ:\r\nУслуга: {}\r\nФотограф: {}\r\nДата: {}\r\nВремя: {}\r\nАдрес: {}\r\n",
                                                        service.name,
                                                        photographer.name,
                                                        session.selected_date.unwrap(),
                                                        time,
                                                        "Москва, ул. Адмирала, д.4");
                    bot.send_message(chat_id, order_string)
                        .await
                        .unwrap();
                    let confirm_string = "Подтвердить заказ?";
                    bot.send_message(chat_id, confirm_string)
                        .reply_markup(ReplyMarkup::InlineKeyboard(key))
                        .await
                        .unwrap();
                },
                _ => {}
            }

            bot.answer_callback_query(q.id).await?;
        } else if data.starts_with("service:") { //todo добавить кнопки доп инфы по услугам
            let service_id = data.split(':').nth(1).unwrap().parse::<i32>().unwrap();
            session.service_id = Some(service_id);
            show_photographers_for_service(bot.clone(), chat_id, &pool, service_id).await;
            bot.answer_callback_query(q.id).await?;
        } else if data.starts_with("photographer:") { //todo добавить кнопки доп инфы по фотографам, в тч ссылка на их порфтолио, добавить "любой фотограф"
            let photographer_id = data.split(':').nth(1).unwrap().parse::<i32>().unwrap();
            session.photographer_id = Some(photographer_id);
            let today_month = chrono::Utc::now().month();
            let toady_year = chrono::Utc::now().year();
            let key = generate_calendar(today_month, toady_year);
            bot.send_message(chat_id, "Выбери дату:")
                    .reply_markup(ReplyMarkup::InlineKeyboard(key))
                    .await
                    .unwrap();

            bot.answer_callback_query(q.id).await?;
        } else if data.starts_with("confirming:") {
            let answer = data.split(":").collect::<Vec<&str>>();
            if answer[1] == "yes" {
                let booking_start = PrimitiveDateTime::new(session.selected_date.unwrap(), session.selected_time_start.unwrap());
                let booking_end = PrimitiveDateTime::new(session.selected_date.unwrap(), session.selected_time_end.unwrap());

                match create_booking(
                    &pool,
                    session.client_id,
                    session.photographer_id.unwrap(),
                    session.service_id.unwrap(),
                    booking_start,
                    booking_end
                ).await {
                    Ok(_) => {
                        bot.send_message(chat_id, "Ваш заказ подтвержден! 🎉")
                            .await
                            .unwrap();
                    }
                    Err(e) => {
                        error!("Error creating booking: {}", e);
                        bot.send_message(chat_id, "Ошибка при создании заказа. Попробуйте еще раз.")
                            .await
                            .unwrap();
                        session.step = UserStep::MainMenu;
                        let buttons: Vec<Vec<KeyboardButton>> = vec![
                            vec![KeyboardButton::new("Выбрать услугу")],
                            vec![KeyboardButton::new("История заказов")],
                        ];
                        let keyboard = KeyboardMarkup::new(buttons).resize_keyboard();
                        bot.send_message(chat_id, "Выбери действие")
                            .reply_markup(ReplyMarkup::Keyboard(keyboard))
                            .await
                            .unwrap();
                    }

                }
            } else { //todo кидать на выбор услуги
                show_services(bot, chat_id, &pool).await;
            }
        } else if data.starts_with("agree") {
            session.agreement = true;
        }
    }
    Ok(())
}

async fn validate_selected_date (date: &str) -> bool {
    let today_year = chrono::Utc::now().year();
    let today_month = chrono::Utc::now().month();
    let today_day = chrono::Utc::now().day();
    let today_string = format!("{}-{:02}-{:02}", today_year, today_month, today_day);
    println!("today_string: {}", today_string);

    let date_format = format_description!("[year]-[month]-[day]");
    let today = Date::parse(today_string.as_str(), &date_format);
    match today {
        Ok(today) => println!("today_parsed: {}", today),
        Err(e) => {
            error!("Error parsing date: {}", e);
            return false;
        }
    }
    let date = Date::parse(date, &date_format).ok().unwrap();
    println!("today: {}, date: {}", today.unwrap(), date);
    if date < today.unwrap() {
        return false;
    }
    true
}

async fn get_free_slots(
    pool: &PgPool,
    photographer_id: i32,
    service_id: i32,
    date: PrimitiveDateTime,
) -> Result<Vec<String>, sqlx::Error> {
    // 1. Получаем длительность услуги
    let duration: i32 = sqlx::query_scalar!(
        "SELECT duration FROM services WHERE id = $1",
        service_id
    )
    .fetch_one(pool)
    .await?;

    // 2. Получаем все бронирования на эту дату
    let start_of_day:PrimitiveDateTime = PrimitiveDateTime::new(date.date(), time!(8:00));
    let end_of_day:PrimitiveDateTime = PrimitiveDateTime::new(date.date(), time!(20:00));

    let bookings = sqlx::query!(
        "SELECT booking_start, booking_end FROM bookings
         WHERE photographer_id = $1
         AND booking_start >= $2 AND booking_end <= $3",
        photographer_id,
        start_of_day,
        end_of_day
    )
    .fetch_all(pool)
    .await?;

    // 3. Строим слоты
    let mut free_slots = vec![];
    let mut current_time = start_of_day.time();
    let end_time = end_of_day.time();
    while current_time < end_time {
        let slot_start = PrimitiveDateTime::new(date.date(), current_time);
        let slot_end = slot_start + time::Duration::minutes(duration as i64);

        // Проверяем, пересекается ли слот с существующими бронированиями
        if !bookings.iter().any(|b| {
            let booking_start = b.booking_start.time();
            let booking_end = b.booking_end.time();
            (slot_start.time() < booking_end) && (slot_end.time() > booking_start)
        }) {
            let start_hour = slot_start.time().hour();
            let start_minute = slot_start.time().minute();
            let end_hour = slot_end.time().hour();
            let end_minute = slot_end.time().minute();
            free_slots.push(format!("{}:{:02}-{}:{:02}", start_hour, start_minute, end_hour, end_minute));
        }

        current_time = current_time + time::Duration::minutes(30);
    }

    Ok(free_slots)
}

// Функции для работы с БД
async fn get_services(pool: &PgPool) -> Vec<Service> {
    sqlx::query_as::<_, Service>("SELECT * FROM services")
        .fetch_all(pool)
        .await
        .unwrap()
}

async fn show_agreement(bot: Bot, chat_id: ChatId) {
    let keyboard = InlineKeyboardMarkup::new(vec![vec![InlineKeyboardButton::callback(
        "Согласен", // Текст кнопки
        "agree",    // Callback-данные, которые бот получит при нажатии
    )]]);
    bot.send_message(chat_id, "Продолжая диалог, вы подтверждаете, что ознакомлены и согласны с правилами фотостудии (URL) и политикой конфиденциальности (URL)")
    .reply_markup(keyboard)
    .await;
}

async fn show_services(bot: Bot, chat_id: ChatId, pool: &PgPool) {
    let services = get_services(pool).await;

    let button: Vec<String> = services
        .iter()
        .map(|s| s.name.clone())
        .collect();
    let action: Vec<String> = services
        .iter()
        .map(|s| format!("{}", s.id))
        .collect();

    let keyboard: InlineKeyboardMarkup = generate_inline_markup("service", button, action);

    bot.send_message(chat_id, "Выбери услугу 📸")
        .reply_markup(ReplyMarkup::InlineKeyboard(keyboard))
        .await
        .unwrap();
}

async fn get_photographers_by_service(pool: &PgPool, service_id: i32) -> Vec<Photographer> {
    sqlx::query_as::<_, Photographer>(
        "SELECT p.* FROM photographers p
         JOIN photographer_services ps ON p.id = ps.photographer_id
         WHERE ps.service_id = $1"
    )
    .bind(service_id)
    .fetch_all(pool)
    .await
    .unwrap()
}

async fn show_photographers_for_service(bot: Bot, chat_id: ChatId, pool: &PgPool, service_id: i32) {
    let photographers = get_photographers_by_service(pool, service_id).await;

    if photographers.is_empty() {
        bot.send_message(chat_id, "Нет доступных фотографов для этой услуги 😢")
            .await
            .unwrap();
        return;
    }

    let buttons = photographers
        .iter()
        .map(|p| p.name.clone())
        .collect();
    let actions = photographers
        .iter()
        .map(|p| format!("{}", p.id))
        .collect();
    let keyboard = generate_inline_markup("photographer", buttons, actions);

    bot.send_message(chat_id, "Выбери фотографа 📷")
        .reply_markup(ReplyMarkup::InlineKeyboard(keyboard))
        .await
        .unwrap();
}

pub fn generate_inline_markup(mark: &str, button: Vec<String>, action: Vec<String>) -> InlineKeyboardMarkup {
    //todo кнопка назад
    let mut keyboard: Vec<Vec<InlineKeyboardButton>> = Vec::new();
    for (i, b) in button.iter().enumerate() {
        let ignore_action = "ignore".to_string();
        let action = action.get(i).unwrap_or(&ignore_action);
        keyboard.push(vec![InlineKeyboardButton::callback(b.clone(), format!("{}:{}", mark, action.clone()))]);
    }
    keyboard.push(vec![InlineKeyboardButton::callback("⟵ Назад", format!("back"))]);
    InlineKeyboardMarkup::new(keyboard)
}

pub fn generate_calendar(month: u32, year: i32) -> InlineKeyboardMarkup {
    let mut keyboard: Vec<Vec<InlineKeyboardButton>> = Vec::new();

    // 1. Заголовок с месяцем и годом
    let month_name = month_name(month);
    keyboard.push(vec![
        InlineKeyboardButton::callback(format!("📅 {} {}", month_name, year), "ignore".to_string())
    ]);

    // 2. Дни недели
    let weekdays = vec!["Пн", "Вт", "Ср", "Чт", "Пт", "Сб", "Вс"];
    keyboard.push(weekdays.into_iter().map(|day| {
        InlineKeyboardButton::callback(day.to_string(), "ignore".to_string())
    }).collect());

    // 3. Дни месяца
    if let Some(first_day) = NaiveDate::from_ymd_opt(year, month, 1) {
        let mut row: Vec<InlineKeyboardButton> = Vec::new();
        let num_days = days_in_month(month, year);

        let shift = (first_day.weekday().num_days_from_monday()) as usize;
        for _ in 0..shift {
            row.push(InlineKeyboardButton::callback(" ".to_string(), "ignore".to_string()));
        }

        for day in 1..=num_days {
            let date = NaiveDate::from_ymd_opt(year, month, day).unwrap();
            let callback = format!("calendar:select:{}", date); // Callback формат

            row.push(InlineKeyboardButton::callback(day.to_string(), callback));

            if row.len() == 7 {
                keyboard.push(row.clone());
                row.clear();
            }
        }

        if !row.is_empty() {
            while row.len() < 7 {
                row.push(InlineKeyboardButton::callback(" ".to_string(), "ignore".to_string()));
            }
            keyboard.push(row);
        }
    }

    // 4. Переключатели месяцев и годов
    keyboard.push(vec![
        InlineKeyboardButton::callback("<< Год", format!("calendar:prev_year:{}", year)),
        InlineKeyboardButton::callback("< Месяц", format!("calendar:prev_month:{}:{}", month, year)),
        InlineKeyboardButton::callback("Месяц >", format!("calendar:next_month:{}:{}", month, year)),
        InlineKeyboardButton::callback("Год >>", format!("calendar:next_year:{}", year)),
    ]);

    InlineKeyboardMarkup::new(keyboard)
}

fn month_name(month: u32) -> &'static str {
    match month {
        1 => "Январь", 2 => "Февраль", 3 => "Март", 4 => "Апрель",
        5 => "Май", 6 => "Июнь", 7 => "Июль", 8 => "Август",
        9 => "Сентябрь", 10 => "Октябрь", 11 => "Ноябрь", 12 => "Декабрь",
        _ => "",
    }
}

fn days_in_month(month: u32, year: i32) -> u32 {
    let next_month = if month == 12 { 1 } else { month + 1 };
    let next_year = if month == 12 { year + 1 } else { year };

    let last_day = NaiveDate::from_ymd_opt(next_year, next_month, 1)
        .unwrap()
        .pred_opt()
        .unwrap();

    last_day.day()
}

async fn create_booking(pool: &PgPool, client_id: i32, photographer_id: i32, service_id: i32, booking_start: PrimitiveDateTime, booking_end: PrimitiveDateTime) -> Result<PgQueryResult, sqlx::Error> {
    sqlx::query!(
        "INSERT INTO bookings (client_id, photographer_id, service_id, booking_start, booking_end, status)
         VALUES ($1, $2, $3, $4, $5, $6)",
        client_id as i32,
        photographer_id,
        service_id,
        booking_start,
        booking_end,
        "confirmed"
    )
    .execute(pool)
    .await
}

async fn check_client (pool: &PgPool, telegram_id: i64) -> Option<Client> {
    sqlx::query_as::<_, Client>("SELECT * FROM clients WHERE telegram_id = $1")
        .bind(telegram_id as i32)
        .fetch_optional(pool)
        .await
        .unwrap()
}
