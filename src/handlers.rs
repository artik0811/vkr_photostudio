use chrono::{Duration, NaiveDate, NaiveDateTime, NaiveTime, Local};
use log::{error};
use teloxide::{prelude::*, types::{CallbackQuery, InlineKeyboardButton, InlineKeyboardMarkup, KeyboardButton, KeyboardMarkup, Message, ReplyMarkup, User, WebAppInfo, MessageId}, RequestError};
use sqlx::{postgres::PgQueryResult, query::{self, Map}, PgPool, Row};
use url::Url;
use core::slice;
use std::{collections::HashMap, vec, error::Error};
use time::{macros::{format_description, time}, Date, Month, PrimitiveDateTime, Time};
use chrono::Datelike;

use crate::models::{Client, Photographer, Service};
extern crate pretty_env_logger;

#[derive(sqlx::FromRow)]
struct BookingInfo {
    id: i32,
    booking_start: PrimitiveDateTime,
    booking_end: PrimitiveDateTime,
    status: String,
    client_name: String,
    service_name: String,
    client_id: Option<i32>,
    photographer_id: Option<i32>,
    service_id: Option<i32>,
    client_phone: Option<String>,  // Это будет username клиента
}

// Структура для хранения сессии пользователя
pub struct UserSession {
    step: UserStep,
    client_id: i32,
    photographer_id: Option<i32>,
    service_id: Option<i32>,
    selected_date: Option<Date>,
    selected_time_start: Option<Time>,
    selected_time_end: Option<Time>,
    agreement: bool,
    user_type: UserType,
    client_name: String,
    client_username: String
}

impl UserSession {
    fn new() -> Self {
        UserSession {
            step: UserStep::Registartion,
            client_id: -1,
            photographer_id: None,
            service_id: None,
            selected_date: None,
            selected_time_start: None,
            selected_time_end: None,
            agreement: false,
            user_type: UserType::Unknown,
            client_name: String::new(),
            client_username: String::new()
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq)]
enum UserType {
    Unknown,
    Client,
    Photographer
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
    // Photographer specific steps
    PhotographerMainMenu,
    ViewSchedule,
    ViewBookings,
    ChangeDescription,
    ChangePortfolio,
    CustomHours,
    // New steps
    ChangeName,
    PersonalCabinet,
    SelectTime,
}

#[derive(Debug, Clone, Copy, PartialEq)]
enum BookingStatus {
    New,
    Confirmed,
    Completed,
    Cancelled
}

impl BookingStatus {
    fn to_string(&self) -> &'static str {
        match self {
            BookingStatus::New => "🆕 Новый",
            BookingStatus::Confirmed => "✅ Подтвержден",
            BookingStatus::Completed => "✅ Выполнен",
            BookingStatus::Cancelled => "❌ Отменен"
        }
    }
}

//todo
/*
    1. Согласие на обработку данных, 
    2. возмонжость отозвать его и после отзыва хранить клиента в архиве
    3. Кнопку "Любой фотограф" после выбора услуги

*/

pub async fn handle_message(msg: Message, bot: Bot, pool: PgPool, user_sessions: &mut HashMap<i64, UserSession>) -> Result<(), Box<dyn Error + Send + Sync>> {
    let chat_id = msg.chat.id;
    let text = msg.text().unwrap_or_else(|| "");

    let session = user_sessions.entry(chat_id.0).or_insert(UserSession::new());
    if let Some(text) = msg.text() {
        match text {
            "/start" => {
                // Сначала проверяем, является ли пользователь фотографом
                if let Some(photographer) = check_photographer(&pool, chat_id.0).await {
                    println!("User {} is a photographer", chat_id.0);
                    session.user_type = UserType::Photographer;
                    session.photographer_id = Some(photographer.id);
                    session.step = UserStep::PhotographerMainMenu;
                    show_photographer_menu(bot.clone(), chat_id).await?;
                    return Ok(());
                }

                // Если не фотограф, проверяем, зарегистрирован ли как клиент
                if let Some(client) = check_client(&pool, chat_id.0).await {
                    println!("User {} is a client", chat_id.0);
                    session.user_type = UserType::Client;
                    session.client_id = client.id;
                    session.step = UserStep::MainMenu;
                    let buttons: Vec<Vec<KeyboardButton>> = vec![
                        vec![KeyboardButton::new("Выбрать услугу")],
                        vec![KeyboardButton::new("Личный кабинет")],
                    ];
                    let keyboard = KeyboardMarkup::new(buttons).resize_keyboard();
                    bot.send_message(chat_id, "Привет! Я бот фотостудии. Как я могу помочь?")
                        .reply_markup(ReplyMarkup::Keyboard(keyboard))
                        .await?;
                    return Ok(());
                }

                // Если ни фотограф, ни клиент - начинаем регистрацию
                session.step = UserStep::Registartion;
                bot.send_message(chat_id, "Привет! Я бот фотостудии. Пожалуйста, введи свое имя:")
                    .await?;
                return Ok(());
            },
            "Выбрать услугу" => {
                if session.client_id == -1 {
                    session.step = UserStep::Start;
                } else {
                    session.step = UserStep::MainMenu;
                }
            },
            "История записей" => {
                if session.client_id == -1 {
                    session.step = UserStep::Start;
                } else {
                    session.step = UserStep::PersonalCabinet;
                }
            },
            "Изменить имя" => {
                session.step = UserStep::ChangeName;
                bot.send_message(chat_id, "Введите новое имя:").await?;
            },
            "Личный кабинет" => {
                session.step = UserStep::PersonalCabinet;
                let buttons: Vec<Vec<KeyboardButton>> = vec![
                        vec![KeyboardButton::new("История записей")],
                        vec![KeyboardButton::new("Изменить имя")],
                        vec![KeyboardButton::new("Отозвать согласие")],
                        vec![KeyboardButton::new("⟵ Назад")],
                    ];
                    let keyboard = KeyboardMarkup::new(buttons).resize_keyboard();
                    bot.send_message(chat_id, "Личный кабинет")
                        .reply_markup(ReplyMarkup::Keyboard(keyboard))
                        .await?;
            }
            "Отозвать согласие" => {
                let keyboard = InlineKeyboardMarkup::new(vec![
                    vec![InlineKeyboardButton::callback("Да, отозвать согласие", "revoke_consent:confirm")],
                    vec![InlineKeyboardButton::callback("Нет, отменить", "revoke_consent:cancel")],
                ]);
                bot.send_message(chat_id, "Вы уверены, что хотите отозвать согласие на обработку данных? Это приведет к удалению вашего аккаунта.")
                    .reply_markup(ReplyMarkup::InlineKeyboard(keyboard))
                    .await?;
            },
            _ => {}
        }
    }

    match session.step {
        UserStep::Registartion => {
            println!("Registartion: {}", text);
            let name = text.to_string();
            let username = msg.from().and_then(|user| user.username.clone());
            
            println!("Registering user with telegram_id: {}, name: {}, username: {:?}", 
                chat_id.0, name, username);
            
            // Проверяем, не является ли пользователь фотографом
            if let Some(photographer) = check_photographer(&pool, chat_id.0).await {
                println!("User {} is already a photographer", chat_id.0);
                session.user_type = UserType::Photographer;
                session.photographer_id = Some(photographer.id);
                session.step = UserStep::PhotographerMainMenu;
                show_photographer_menu(bot.clone(), chat_id).await?;
                return Ok(());
            }
            
            // Сохраняем имя и username во временные поля сессии
            session.client_id = -1;
            session.user_type = UserType::Client;
            session.client_name = name.clone();
            session.client_username = username.unwrap().clone();
            
            // Показываем согласие на обработку данных
            let keyboard = InlineKeyboardMarkup::new(vec![vec![InlineKeyboardButton::callback(
                "Согласен",
                "agree",
            )]]);
            bot.send_message(chat_id, format!("{}, вы подтверждаете, что ознакомлены и согласны с правилами фотостудии (URL) и политикой конфиденциальности (URL)", name))
                .reply_markup(ReplyMarkup::InlineKeyboard(keyboard))
                .await?;
            
            // Сохраняем данные для последующего использования
            session.step = UserStep::Start;
        }

        UserStep::Start => {
            println!("Start : {}", text);
            println!("{}, {}", chat_id.0, msg.chat.id);
            // Check if user is a photographer
            if let Some(photographer) = check_photographer(&pool, chat_id.0).await {
                session.user_type = UserType::Photographer;
                session.photographer_id = Some(photographer.id);
                session.step = UserStep::PhotographerMainMenu;
                show_photographer_menu(bot.clone(), chat_id).await?;
                return Ok(());
            }

            // If not a photographer, proceed with client flow
            let client = check_client(&pool, chat_id.0).await;
            let buttons: Vec<Vec<KeyboardButton>> = vec![
                vec![KeyboardButton::new("Выбрать услугу")],
                vec![KeyboardButton::new("Личный кабинет")],
            ];

            if client.is_some() {
                let keyboard = KeyboardMarkup::new(buttons).resize_keyboard();
                session.user_type = UserType::Client;
                bot.send_message(chat_id, "Привет! Я бот фотостудии. Как я могу помочь?")
                    .reply_markup(ReplyMarkup::Keyboard(keyboard))
                    .await
                    .unwrap();
                session.step = UserStep::MainMenu;
                session.client_id = client.unwrap().id;
            } else {
                bot.send_message(chat_id, "Привет! Я бот фотостудии. Пожалуйста, введи свое имя:")
                    .await
                    .unwrap();
                return Ok(());
            }
            session.step = UserStep::Registartion;
        }

        UserStep::PhotographerMainMenu => {
            if session.user_type != UserType::Photographer {
                bot.send_message(chat_id, "Неизвестная команда")
                    .await
                    .unwrap();
                return Ok(());
            }
            if text == "Моё расписание" {
                if let Some(photographer_id) = session.photographer_id {
                    show_photographer_schedule(bot.clone(), &msg, &pool, photographer_id).await?;
                }
            } else if text == "Мои записи" {
                if let Some(photographer_id) = session.photographer_id {
                    show_photographer_bookings(bot.clone(), chat_id, &pool, photographer_id).await?;
                }
            } else if text == "Изменить портфолио" {
                session.step = UserStep::ChangePortfolio;
                bot.send_message(chat_id, "Пришлите новую ссылку на портфолио в виде \"https://www.google.com/\"")
                    .await
                    .unwrap();
            } else if text == "Изменить свое описание" {
                session.step = UserStep::ChangeDescription;
                bot.send_message(chat_id, "Пришлите новое описание одним сообщением")
                    .await
                    .unwrap();
            }
        }

        UserStep::ChangePortfolio => {
            if session.user_type != UserType::Photographer {
                bot.send_message(chat_id, "Неизвестная команда")
                    .await
                    .unwrap();
                return Ok(());
            }
            if let Some(text) = msg.text() {
                sqlx::query!(
                    "UPDATE photographers SET portfolio_url = $1 WHERE id = $2",
                    text,
                    session.photographer_id
                )
                .execute(&pool)
                .await?;
                bot.send_message(chat_id, "Портфолио обновлено!")
                    .await
                    .unwrap();
            } else {
                bot.send_message(chat_id, "Пожалуйста, отправьте новую ссылку")
                    .await
                    .unwrap();
            }
        }

        UserStep::ChangeDescription => {
            if session.user_type != UserType::Photographer {
                bot.send_message(chat_id, "Неизвестная команда")
                    .await
                    .unwrap();
                return Ok(());
            }
            if let Some(text) = msg.text() {
                sqlx::query!(
                    "UPDATE photographers SET description = $1 WHERE id = $2",
                    text,
                    session.photographer_id
                )
                .execute(&pool)
                .await?;
                bot.send_message(chat_id, "Описание обновлено!")
                    .await
                    .unwrap();
            } else {
                bot.send_message(chat_id, "Пожалуйста, отправьте новое описание")
                    .await
                    .unwrap();
            }
        }

        UserStep::SelectingService => {
            println!("SelectingService: {}", text);
        }

        UserStep::MainMenu => {
            println!("Main Menu: {}", text);
            if text == "Выбрать услугу" {
                session.step = UserStep::SelectingService;
                show_services(bot.clone(), chat_id, &pool).await;
            } else if text == "Личный кабинет" {
                session.step = UserStep::PersonalCabinet;
                let buttons: Vec<Vec<KeyboardButton>> = vec![
                    vec![KeyboardButton::new("История записей")],
                    vec![KeyboardButton::new("Изменить имя")],
                    vec![KeyboardButton::new("Отозвать согласие")],
                    vec![KeyboardButton::new("⟵ Назад")],
                ];
                let keyboard = KeyboardMarkup::new(buttons).resize_keyboard();
                bot.send_message(chat_id, "Личный кабинет")
                    .reply_markup(ReplyMarkup::Keyboard(keyboard))
                    .await?;
            }
        },

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
            if session.client_id != -1 {
                show_client_bookings(bot.clone(), chat_id, pool, session.client_id, 0, session, msg).await?;
                session.step = UserStep::MainMenu;
            } else {
                bot.send_message(chat_id, "Пожалуйста, сначала зарегистрируйтесь")
                    .await
                    .unwrap();
                session.step = UserStep::Start;
            }
        }

        UserStep::Payment => {
            println!("Payment: {}", text);
        },
        UserStep::ViewSchedule => {
            if let Some(photographer_id) = session.photographer_id {
                show_photographer_schedule(bot.clone(), &msg, &pool, photographer_id).await?;
            }
        },
        UserStep::ViewBookings => {
            if let Some(photographer_id) = session.photographer_id {
                show_photographer_bookings(bot.clone(), chat_id, &pool, photographer_id).await?;
            }
        },
        UserStep::CustomHours => {
            if let Some(text) = msg.text() {
                let parts: Vec<&str> = text.split('-').collect();
                if parts.len() == 2 {
                    let start_parts: Vec<&str> = parts[0].split(':').collect();
                    let end_parts: Vec<&str> = parts[1].split(':').collect();
                    
                    if start_parts.len() == 2 && end_parts.len() == 2 {
                        if let (Ok(start_hour), Ok(end_hour)) = (start_parts[0].parse::<i32>(), end_parts[0].parse::<i32>()) {
                            if let Some(date) = session.selected_date {
                                if let Err(e) = save_working_hours(&pool, session.photographer_id.unwrap(), date, start_hour, end_hour).await {
                                    error!("Error saving working hours: {}", e);
                                    bot.send_message(chat_id, "Произошла ошибка при сохранении рабочих часов")
                                        .await
                                        .unwrap();
                                } else {
                                    bot.send_message(chat_id, "Рабочие часы успешно сохранены")
                                        .await
                                        .unwrap();
                                    session.step = UserStep::PhotographerMainMenu;
                                    show_photographer_menu(bot.clone(), chat_id).await?;
                                }
                            }
                        }
                    }
                }
                bot.send_message(chat_id, "Неверный формат. Используйте формат ЧЧ:ЧЧ-ЧЧ:ЧЧ (например, 9:00-18:00)")
                    .await
                    .unwrap();
            }
        },
        UserStep::PersonalCabinet => {
            match text {
                "История записей" => {
                    let bookings = sqlx::query!(
                        r#"
                        SELECT b.*, p.name as photographer_name, s.name as service_name
                        FROM bookings b
                        JOIN photographers p ON b.photographer_id = p.id
                        JOIN services s ON b.service_id = s.id
                        WHERE b.client_id = $1
                        ORDER BY b.booking_start DESC
                        "#,
                        session.client_id
                    )
                    .fetch_all(&pool)
                    .await?;

                    if bookings.is_empty() {
                        bot.send_message(chat_id, "У вас пока нет записей").await?;
                    } else {
                        let bookings_per_page = 3;
                        let total_pages = (bookings.len() + bookings_per_page - 1) / bookings_per_page;
                        let current_page = 0;

                        let start_idx = current_page * bookings_per_page;
                        let end_idx = std::cmp::min(start_idx + bookings_per_page, bookings.len());
                        let page_bookings = &bookings[start_idx..end_idx];

                        let mut message = String::from("📋 История ваших записей:\n\n");
                        let mut keyboard = vec![];

                        for booking in page_bookings {
                            let date_format = format_description!("[day].[month].[year]");
                            let time_format = format_description!("[hour]:[minute]");
                            
                            let date = booking.booking_start.format(&date_format).unwrap();
                            let start_time = booking.booking_start.format(&time_format).unwrap();
                            let end_time = booking.booking_end.format(&time_format).unwrap();
                            
                            let status = match booking.status.as_str() {
                                "new" => "🆕 Новый",
                                "confirmed" => "✅ Подтвержден",
                                "completed" => "✅ Выполнен",
                                "cancelled" => "❌ Отменен",
                                _ => booking.status.as_str()
                            };
                            
                            message.push_str(&format!(
                                "*Запись №{}*\n*Дата:* {}\n*Время:* {} - {}\n*Фотограф:* {}\n*Услуга:* {}\n*Статус:* {}\n\n",
                                booking.id,
                                date,
                                start_time,
                                end_time,
                                booking.photographer_name,
                                booking.service_name,
                                status
                            ));
                        }

                        if bookings.len() > bookings_per_page {
                            let mut nav_buttons = vec![];
                            if current_page > 0 {
                                nav_buttons.push(InlineKeyboardButton::callback("⬅️ Назад", format!("client_bookings:{}", current_page - 1)));
                            }
                            nav_buttons.push(InlineKeyboardButton::callback(
                                format!("📄 {}/{}", current_page + 1, total_pages),
                                "ignore".to_string(),
                            ));
                            if current_page < total_pages - 1 {
                                nav_buttons.push(InlineKeyboardButton::callback("Вперед ➡️", format!("client_bookings:{}", current_page + 1)));
                            }
                            keyboard.push(nav_buttons);
                        }

                        let keyboard = InlineKeyboardMarkup::new(keyboard);
                        bot.send_message(chat_id, message)
                            .parse_mode(teloxide::types::ParseMode::Markdown)
                            .reply_markup(ReplyMarkup::InlineKeyboard(keyboard))
                            .await?;
                    }

                    // Возвращаемся в личный кабинет
                    let buttons: Vec<Vec<KeyboardButton>> = vec![
                        vec![KeyboardButton::new("История записей")],
                        vec![KeyboardButton::new("Изменить имя")],
                        vec![KeyboardButton::new("Отозвать согласие")],
                        vec![KeyboardButton::new("⟵ Назад")],
                    ];
                    let keyboard = KeyboardMarkup::new(buttons).resize_keyboard();
                    bot.send_message(chat_id, "Личный кабинет")
                        .reply_markup(ReplyMarkup::Keyboard(keyboard))
                        .await?;
                },
                "Изменить имя" => {
                    session.step = UserStep::ChangeName;
                    bot.send_message(chat_id, "Введите новое имя:").await?;
                },
                "Отозвать согласие" => {
                    let keyboard = InlineKeyboardMarkup::new(vec![
                        vec![InlineKeyboardButton::callback("Да, отозвать согласие", "revoke_consent:confirm")],
                        vec![InlineKeyboardButton::callback("Нет, отменить", "revoke_consent:cancel")],
                    ]);
                    bot.send_message(chat_id, "Вы уверены, что хотите отозвать согласие на обработку данных? Это приведет к удалению вашего аккаунта.")
                        .reply_markup(ReplyMarkup::InlineKeyboard(keyboard))
                        .await?;
                },
                "⟵ Назад" => {
                    session.step = UserStep::MainMenu;
                    let buttons: Vec<Vec<KeyboardButton>> = vec![
                        vec![KeyboardButton::new("Выбрать услугу")],
                        vec![KeyboardButton::new("Личный кабинет")],
                    ];
                    let keyboard = KeyboardMarkup::new(buttons).resize_keyboard();
                    bot.send_message(chat_id, "Выбери действие")
                        .reply_markup(ReplyMarkup::Keyboard(keyboard))
                        .await?;
                },
                _ => {}
            }
        },
        UserStep::ChangeName => {
            let new_name = text.to_string();
            if new_name == "Изменить имя" {
                return Ok(());
            }
            
            if new_name.len() < 2 {
                bot.send_message(chat_id, "Имя должно содержать минимум 2 символа. Попробуйте еще раз:").await?;
                return Ok(());
            }
            
            sqlx::query!(
                "UPDATE clients SET name = $1 WHERE telegram_id = $2",
                new_name,
                chat_id.0 as i32
            )
            .execute(&pool)
            .await?;
            
            bot.send_message(chat_id, "Имя успешно изменено!").await?;
            session.step = UserStep::MainMenu;
            
            let buttons: Vec<Vec<KeyboardButton>> = vec![
                vec![KeyboardButton::new("Выбрать услугу")],
                vec![KeyboardButton::new("Личный кабинет")],
            ];
            let keyboard = KeyboardMarkup::new(buttons).resize_keyboard();
            bot.send_message(chat_id, "Выбери действие")
                .reply_markup(ReplyMarkup::Keyboard(keyboard))
                .await?;
        },
        UserStep::SelectTime => {
            if let Some(date_str) = text.split('_').nth(1) {
                if let Ok(date) = NaiveDate::parse_from_str(date_str, "%Y-%m-%d") {
                    let today = Date::from_calendar_date(
                        Local::now().year(),
                        Month::try_from(Local::now().month() as u8).unwrap(),
                        Local::now().day() as u8
                    ).unwrap();
                    let selected_date = Date::from_calendar_date(
                        date.year(),
                        Month::try_from(date.month() as u8).unwrap(),
                        date.day() as u8
                    ).unwrap();
                    
                    if selected_date < today {
                        bot.send_message(
                            chat_id,
                            "Нельзя выбрать дату в прошлом. Пожалуйста, выберите другую дату.",
                        )
                        .await?;
                        return Ok(());
                    }
                    
                    session.selected_date = Some(selected_date);
                    session.step = UserStep::SelectTime;
                }
            }
        },
    }
    Ok(())
}

pub async fn handle_callback_query(q: CallbackQuery, bot: Bot, msg: Message, pool: PgPool, user_sessions: &mut HashMap<i64, UserSession>) -> Result<(), Box<dyn Error + Send + Sync>> {
    println!("Callback query: {:?}", q.data.clone().unwrap());
    let chat_id: ChatId = msg.chat.id;
    let _text: &str = msg.text().unwrap_or_else(|| "");
    let session: &mut UserSession = user_sessions.entry(chat_id.0).or_insert(UserSession::new());

    if let Some(data) = q.data.clone() {
        match data.as_str() {
            "upcoming_bookings" => {
                if let Some(photographer_id) = session.photographer_id {
                    println!("Processing upcoming_bookings for photographer_id: {}", photographer_id);
                    
                    let bookings = sqlx::query_as!(
                        BookingInfo,
                        r#"
                        SELECT 
                            b.id,
                            b.booking_start,
                            b.booking_end,
                            b.status,
                            c.name as client_name,
                            s.name as service_name,
                            b.client_id,
                            b.photographer_id,
                            b.service_id,
                            c.username as client_phone
                        FROM bookings b 
                        JOIN clients c ON b.client_id = c.id 
                        JOIN services s ON b.service_id = s.id 
                        WHERE b.photographer_id = $1 
                        AND b.booking_start >= CURRENT_TIMESTAMP
                        ORDER BY b.booking_start ASC
                        "#,
                        photographer_id
                    )
                    .fetch_all(&pool)
                    .await?;

                    println!("Found {} bookings", bookings.len());

                    if bookings.is_empty() {
                        if let Some(msg) = q.message.clone() {
                            bot.edit_message_text(chat_id, msg.id(), "У вас нет предстоящих записей").await?;
                        }
                        return Ok(());
                    }

                    let bookings_per_page = 3;
                    let total_pages = (bookings.len() + bookings_per_page - 1) / bookings_per_page;
                    let current_page = 0;

                    let start_idx = current_page * bookings_per_page;
                    let end_idx = std::cmp::min(start_idx + bookings_per_page, bookings.len());
                    let page_bookings = &bookings[start_idx..end_idx];

                    let mut message = String::from("📅 Предстоящие записи:\n\n");
                    let mut keyboard = vec![];

                    for booking in page_bookings {
                        let date_format = format_description!("[day].[month].[year]");
                        let time_format = format_description!("[hour]:[minute]");
                        
                        let date = booking.booking_start.format(&date_format).unwrap();
                        let start_time = booking.booking_start.format(&time_format).unwrap();
                        let end_time = booking.booking_end.format(&time_format).unwrap();
                        
                        let status = match booking.status.as_str() {
                            "new" => "🆕 Новый",
                            "confirmed" => "✅ Подтвержден",
                            "completed" => "✅ Выполнен",
                            "cancelled" => "❌ Отменен",
                            _ => booking.status.as_str()
                        };
                        
                        message.push_str(&format!(
                            "*Запись №{}*\n*Дата:* {}\n*Время:* {} - {}\n*Клиент:* {}\n*Услуга:* {}\n*Статус:* {}\n\n",
                            booking.id,
                            date,
                            start_time,
                            end_time,
                            booking.client_name,
                            booking.service_name,
                            status
                        ));
                        let mut booking_buttons = vec![
                            InlineKeyboardButton::callback(
                                format!("🔢 #{}", booking.id),
                                "ignore".to_string()
                            ),
                        ];
                        if let Some(username) = &booking.client_phone {
                            if !username.is_empty() {
                                let url = format!("https://t.me/{}", username);
                                match Url::parse(&url) {
                                    Ok(parsed_url) => {
                                        booking_buttons.push(InlineKeyboardButton::url(
                                            "📞 Связаться".to_string(),
                                            parsed_url
                                        ));
                                    },
                                    Err(e) => {
                                        println!("Error parsing URL for username {}: {}", username, e);
                                    }
                                }
                            }
                        }


                        // Добавляем кнопки в зависимости от статуса записи
                        if booking.status == "confirmed" {
                            booking_buttons.push(InlineKeyboardButton::callback(
                                "✅ Завершить".to_string(),
                                format!("complete_booking:{}", booking.id)
                            ));
                            booking_buttons.push(InlineKeyboardButton::callback(
                                "❌ Отменить".to_string(),
                                format!("reject_booking:{}", booking.id)
                            ));
                        }
                        keyboard.push(booking_buttons);
                    }

                    if bookings.len() > bookings_per_page {
                        let mut nav_buttons = vec![];
                        if current_page > 0 {
                            nav_buttons.push(InlineKeyboardButton::callback("⬅️ Назад", format!("page_upcoming:{}", current_page - 1)));
                        }
                        nav_buttons.push(InlineKeyboardButton::callback(
                            format!("📄 {}/{}", current_page + 1, total_pages),
                            "ignore".to_string(),
                        ));
                        if current_page < total_pages - 1 {
                            nav_buttons.push(InlineKeyboardButton::callback("Вперед ➡️", format!("page_upcoming:{}", current_page + 1)));
                        }
                        keyboard.push(nav_buttons);
                    }

                    let keyboard = InlineKeyboardMarkup::new(keyboard);
                    if let Some(msg) = q.message.clone() {
                        bot.edit_message_text(chat_id, msg.id(), message)
                            .parse_mode(teloxide::types::ParseMode::Markdown)
                            .reply_markup(keyboard)
                            .await?;
                    }
                }
            },
            "all_bookings" => {
                if let Some(photographer_id) = session.photographer_id {
                    let bookings = sqlx::query_as!(
                        BookingInfo,
                        r#"
                        SELECT 
                            b.id,
                            b.booking_start,
                            b.booking_end,
                            b.status,
                            c.name as client_name,
                            s.name as service_name,
                            b.client_id,
                            b.photographer_id,
                            b.service_id,
                            c.username as client_phone
                        FROM bookings b 
                        JOIN clients c ON b.client_id = c.id 
                        JOIN services s ON b.service_id = s.id 
                        WHERE b.photographer_id = $1 
                        ORDER BY b.booking_start DESC
                        "#,
                        photographer_id
                    )
                    .fetch_all(&pool)
                    .await?;

                    if bookings.is_empty() {
                        if let Some(msg) = q.message.clone() {
                            bot.edit_message_text(chat_id, msg.id(), "У вас нет записей").await?;
                        }
                        return Ok(());
                    }

                    let bookings_per_page = 3;
                    let total_pages = (bookings.len() + bookings_per_page - 1) / bookings_per_page;
                    let current_page = 0;

                    let start_idx = current_page * bookings_per_page;
                    let end_idx = std::cmp::min(start_idx + bookings_per_page, bookings.len());
                    let page_bookings = &bookings[start_idx..end_idx];

                    let mut message = String::from("📋 Все записи:\n\n");
                    let mut keyboard = vec![];

                    for booking in page_bookings {
                        let date_format: &[time::format_description::BorrowedFormatItem<'_>] = format_description!("[day].[month].[year]");
                        let time_format = format_description!("[hour]:[minute]");
                        
                        let date = booking.booking_start.format(&date_format).unwrap();
                        let start_time = booking.booking_start.format(&time_format).unwrap();
                        let end_time = booking.booking_end.format(&time_format).unwrap();
                        
                        let status = match booking.status.as_str() {
                            "new" => "🆕 Новый",
                            "confirmed" => "✅ Подтвержден",
                            "completed" => "✅ Выполнен",
                            "cancelled" => "❌ Отменен",
                            _ => booking.status.as_str()
                        };
                        
                        message.push_str(&format!(
                            "*Запись №{}*\n*Дата:* {}\n*Время:* {} - {}\n*Клиент:* {}\n*Услуга:* {}\n*Статус:* {}\n\n",
                            booking.id,
                            date,
                            start_time,
                            end_time,
                            booking.client_name,
                            booking.service_name,
                            status
                        ));
                        let mut booking_buttons = vec![
                            InlineKeyboardButton::callback(
                                format!("🔢 #{}", booking.id),
                                "ignore".to_string()
                            ),
                        ];
                        if let Some(username) = &booking.client_phone {
                            if !username.is_empty() {
                                let url = format!("https://t.me/{}", username);
                                match Url::parse(&url) {
                                    Ok(parsed_url) => {
                                        booking_buttons.push(InlineKeyboardButton::url(
                                            "📞 Связаться".to_string(),
                                            parsed_url
                                        ));
                                    },
                                    Err(e) => {
                                        println!("Error parsing URL for username {}: {}", username, e);
                                    }
                                }
                            }
                        }
                        keyboard.push(booking_buttons);
                    }

                    if bookings.len() > bookings_per_page {
                        let mut nav_buttons = vec![];
                        if current_page > 0 {
                            nav_buttons.push(InlineKeyboardButton::callback("⬅️ Назад", format!("page_all:{}", current_page - 1)));
                        }
                        nav_buttons.push(InlineKeyboardButton::callback(
                            format!("📄 {}/{}", current_page + 1, total_pages),
                            "ignore".to_string(),
                        ));
                        if current_page < total_pages - 1 {
                            nav_buttons.push(InlineKeyboardButton::callback("Вперед ➡️", format!("page_all:{}", current_page + 1)));
                        }
                        keyboard.push(nav_buttons);
                    }

                    let keyboard = InlineKeyboardMarkup::new(keyboard);
                    
                    if let Some(msg) = q.message.clone() {
                        bot.edit_message_text(chat_id, msg.id(), message)
                            .parse_mode(teloxide::types::ParseMode::Markdown)
                            .reply_markup(keyboard)
                            .await?;
                    }
                }
            },
            "agree" => {
                session.agreement = true;
                
                let name = session.client_name.clone();
                let username = session.client_username.clone();

                // Сохраняем данные клиента
                sqlx::query("INSERT INTO clients (telegram_id, name, username) VALUES ($1, $2, $3) ON CONFLICT (telegram_id) DO UPDATE SET name = $2, username = $3")
                    .bind(chat_id.0 as i32)
                    .bind(name)
                    .bind(username)
                    .execute(&pool)
                    .await
                    .unwrap();
                
                // Получаем ID клиента
                session.client_id = sqlx::query_scalar("SELECT id FROM clients WHERE telegram_id = $1")
                    .bind(chat_id.0 as i32)
                    .fetch_one(&pool)
                    .await
                    .unwrap();
                
                let buttons: Vec<Vec<KeyboardButton>> = vec![
                    vec![KeyboardButton::new("Выбрать услугу")],
                    vec![KeyboardButton::new("Личный кабинет")],
                ];
                let keyboard = KeyboardMarkup::new(buttons).resize_keyboard();
                bot.send_message(chat_id, "Спасибо за согласие! Теперь вы можете пользоваться всеми функциями бота.")
                    .reply_markup(ReplyMarkup::Keyboard(keyboard))
                    .await?;
                
                session.step = UserStep::MainMenu;
            },
            _ if data.starts_with("calendar:") => {
                let parts: Vec<&str> = data.split(':').collect();
                match parts.as_slice() {
                        ["calendar", "select", date] => {
                            let date_format = format_description!("[year]-[month]-[day]");
                            if let Ok(selected_date) = Date::parse(date, &date_format) {
                                let today = Date::from_calendar_date(
                                    Local::now().year(),
                                    Month::try_from(Local::now().month() as u8).unwrap(),
                                    Local::now().day() as u8
                                ).unwrap();
                                
                                if selected_date < today {
                                    bot.send_message(chat_id, "Нельзя выбрать дату в прошлом. Пожалуйста, выберите другую дату.")
                                        .await?;
                                    return Ok(());
                                }
                                
                                session.selected_date = Some(selected_date);
                                
                                if session.user_type == UserType::Photographer {
                                    // Для фотографов показываем настройку рабочего времени
                                    if let Some((start_hour, end_hour)) = get_working_hours(&pool, session.photographer_id.unwrap(), selected_date).await {
                                        let message = format!(
                                            "Текущие рабочие часы на {}: {}:00-{}:00\n\nВыберите новые рабочие часы:",
                                            selected_date,
                                            start_hour,
                                            end_hour
                                        );
                                        add_working_day(bot.clone(), chat_id, &pool, session.photographer_id.unwrap(), selected_date).await?;
                                    } else {
                                        add_working_day(bot.clone(), chat_id, &pool, session.photographer_id.unwrap(), selected_date).await?;
                                    }
                                } else {
                                    // Для клиентов показываем доступные слоты
                                    if let Some(service_id) = session.service_id {
                                        if let Some(photographer_id) = session.photographer_id {
                                            // Если выбран конкретный фотограф
                                            if let Some((start_hour, end_hour)) = get_working_hours(&pool, photographer_id, selected_date).await {
                                                if start_hour > 0 && end_hour > 0 {
                                                    let date_time = PrimitiveDateTime::new(selected_date, time!(0:00));
                                                    match get_free_slots(&pool, photographer_id, service_id, date_time).await {
                                                        Ok(slots) => {
                                                            if slots.is_empty() {
                                                                bot.edit_message_text(chat_id, msg.id, "На выбранную дату нет свободных слотов")
                                                                    .await?;
                                                            } else {
                                                                show_time_slots(bot.clone(), chat_id, slots, msg.id).await?;
                                                            }
                                                        }
                                                        Err(e) => {
                                                            error!("Ошибка при получении свободных слотов: {}", e);
                                                            bot.send_message(chat_id, "Произошла ошибка при получении свободных слотов")
                                                                .await?;
                                                        }
                                                    }
                                                } else {
                                                    bot.send_message(chat_id, "На выбранную дату фотограф не работает. Пожалуйста, выберите другую дату.")
                                                        .await?;
                                                }
                                            } else {
                                                bot.send_message(chat_id, "На выбранную дату фотограф не работает. Пожалуйста, выберите другую дату.")
                                                    .await?;
                                            }
                                        } else {
                                            // Если выбран "любой фотограф"
                                            let date_time = PrimitiveDateTime::new(selected_date, time!(0:00));
                                            match get_available_photographers(&pool, service_id, date_time).await {
                                                Ok(slots) => {
                                                    if slots.is_empty() {
                                                        bot.edit_message_text(chat_id, msg.id, "На выбранную дату нет свободных слотов")
                                                            .await?;
                                                    } else {
                                                        show_time_slots(bot.clone(), chat_id, slots, msg.id).await?;
                                                    }
                                                }
                                                Err(e) => {
                                                    error!("Ошибка при получении свободных слотов: {}", e);
                                                    bot.send_message(chat_id, "Произошла ошибка при получении свободных слотов")
                                                        .await?;
                                                }
                                            }
                                        }
                                    }
                                }
                            }
                        },
                    ["calendar", "next_month", current_month, current_year] => {
                        let mut month: u32 = current_month.parse().unwrap();
                        let mut year: i32 = current_year.parse().unwrap();
                        if month == 12 {
                            month = 1;
                            year += 1;
                        } else {
                            month += 1;
                        }
                            let new_calendar = generate_calendar(month, year, &pool, session.photographer_id.unwrap(), session.user_type).await;

                        if let Some(msg) = q.message.clone() {
                            bot.edit_message_reply_markup(msg.chat().id, msg.id())
                                .reply_markup(new_calendar)
                                .await?;
                        }
                        },
                    ["calendar", "prev_month", current_month, current_year] => {
                        let mut month: u32 = current_month.parse().unwrap();
                        let mut year: i32 = current_year.parse().unwrap();
                        if month == 1 {
                            month = 12;
                            year -= 1;
                        } else {
                            month -= 1;
                        }

                            let new_calendar = generate_calendar(month, year, &pool, session.photographer_id.unwrap(), session.user_type).await;

                        if let Some(msg) = q.message.clone() {
                            bot.edit_message_reply_markup(msg.chat().id, msg.id())
                                .reply_markup(new_calendar)
                                .await?;
                        }
                        },
                    _ => {}
                }
            },
            _ if data.starts_with("photographer_info:") => {
                let photographer_id = data.split(':').nth(1).unwrap().parse::<i32>().unwrap();
                let photographer = sqlx::query_as::<_, Photographer>(
                    "SELECT * FROM photographers WHERE id = $1"
                )
                .bind(photographer_id)
                .fetch_one(&pool)
                .await?;            
                let mut message = format!(
                    "*Информация о фотографе*\n\n\
                    👤 *Имя:* {}\n\
                    {}\n\n",
                    photographer.name,
                    photographer.description.unwrap_or_else(|| "Нет описания".to_string())
                );

                let mut keyboard: Vec<Vec<InlineKeyboardButton>> = Vec::new();
                let protfolio = photographer.portfolio_url;

                if protfolio.is_some() {
                    let portfolio_info = Url::parse(protfolio.unwrap().as_str())?;
                    keyboard.push(vec![
                        InlineKeyboardButton::web_app("Посмотреть портфолио", WebAppInfo { url: portfolio_info })
                    ]);
                }
                keyboard.push(vec![InlineKeyboardButton::callback(
                    "⟵ Назад к фотографам".to_string(),
                    "back_to_photographers".to_string()
                )]);
                let keyboard = InlineKeyboardMarkup::new(keyboard);

                bot.edit_message_text(chat_id, msg.id, message)
                                    .parse_mode(teloxide::types::ParseMode::Markdown)
                                    .reply_markup(keyboard)
                                    .await?;
            },
            _ if data.starts_with("time-") => {
                println!("Time selected: {}", data);
                    let times = data.split("-").collect::<Vec<&str>>();
                let format = format_description!("[hour]:[minute]");
                    match times.as_slice() {
                        ["time", start, end] => {
                            session.selected_time_start = Some(Time::parse(start, &format).unwrap());
                            session.selected_time_end = Some(Time::parse(end, &format).unwrap());

                            let time: String = format!("{}:{:02}-{}:{:02}", 
                                session.selected_time_start.unwrap().hour(), 
                                session.selected_time_start.unwrap().minute(), 
                                session.selected_time_end.unwrap().hour(), 
                                session.selected_time_end.unwrap().minute()
                            );
                            
                        let service = sqlx::query_as::<_, Service>(
                            "SELECT * FROM services WHERE id = $1"
                        )
                        .bind(session.service_id.unwrap())
                        .fetch_one(&pool)
                            .await?;

                            // Если выбран "любой фотограф", находим свободного фотографа
                            let photographer = if session.photographer_id.is_none() {
                                let date_time = PrimitiveDateTime::new(session.selected_date.unwrap(), session.selected_time_start.unwrap());
                                match find_available_photographer(&pool, session.service_id.unwrap(), date_time).await {
                                    Ok(Some(photographer)) => photographer,
                                    Ok(None) => {
                                        if let Some(msg) = q.message.clone() {
                                            bot.edit_message_text(chat_id, msg.id(), "К сожалению, на выбранное время нет свободных фотографов. Пожалуйста, выберите другое время.").await?;
                                        }
                                        return Ok(());
                                    },
                                    Err(e) => {
                                        error!("Error finding available photographer: {}", e);
                                        if let Some(msg) = q.message.clone() {
                                            bot.edit_message_text(chat_id, msg.id(), "Произошла ошибка при поиске фотографа. Пожалуйста, попробуйте позже.").await?;
                                        }
                                        return Ok(());
                                    }
                                }
                            } else {
                                sqlx::query_as::<_, Photographer>(
                            "SELECT * FROM photographers WHERE id = $1",
                        )
                        .bind(session.photographer_id.unwrap())
                        .fetch_one(&pool)
                                .await?
                            };

                        let confirm_button: Vec<String> = vec!["Подтвердить".to_string(), "Изменить".to_string()];
                        let confirm_action: Vec<String> = vec!["yes".to_string(), "no".to_string()];
                            let key: InlineKeyboardMarkup = generate_inline_markup("confirming", confirm_button, confirm_action);
                            
                            let order_string = format!(
                                "*Ваша запись:*\r\n\
                                *Услуга:* {}\r\n\
                                *Фотограф:* {}\r\n\
                                *Дата:* {} {} {}\r\n\
                                *Время:* {}\r\n\
                                *Стоимость:* {} *рублей*\r\n\
                                *Адрес:* {}\r\n",
                                                            service.name,
                                                            photographer.name,
                                                            session.selected_date.unwrap().day(), month_name_from_month(session.selected_date.unwrap().month()), session.selected_date.unwrap().year(),
                                                            time,
                                                            service.cost,
                                "Москва, ул. Адмирала, д.4"
                            );
                            if let Some(msg) = q.message.clone() {
                                bot.edit_message_text(chat_id, msg.id(), order_string)
                                    .parse_mode(teloxide::types::ParseMode::Markdown)
                                    .reply_markup(key)
                                    .await?;
                            }
                    },
                    _ => {}
                }
            },
            _ if data.starts_with("service:") => {
                let service_id = data.split(':').nth(1).unwrap().parse::<i32>().unwrap();
                session.service_id = Some(service_id);
                show_photographers_for_service(bot.clone(), chat_id, &pool, service_id, msg.clone()).await;
            },
            _ if data.starts_with("photographer:") => {
                let photographer_id = data.split(':').nth(1).unwrap();
                if photographer_id == "any" {
                    session.photographer_id = None;
                } else {
                    let photographer_id = photographer_id.parse::<i32>().unwrap();
                    session.photographer_id = Some(photographer_id);
                }
                
                if let Some(msg) = q.message.clone() {
                    let today_month = chrono::Utc::now().month();
                    let today_year = chrono::Utc::now().year();
                    let key = generate_calendar(today_month, today_year, &pool, session.photographer_id.unwrap_or(-1), UserType::Client).await;
                    
                    bot.edit_message_text(chat_id, msg.id(), "Выбери дату:")
                        .reply_markup(key)
                        .await?;
                }
            },
            _ if data.starts_with("confirming:") => {
            let answer = data.split(":").collect::<Vec<&str>>();
            if answer[1] == "yes" {
                let booking_start = PrimitiveDateTime::new(session.selected_date.unwrap(), session.selected_time_start.unwrap());
                let booking_end = PrimitiveDateTime::new(session.selected_date.unwrap(), session.selected_time_end.unwrap());

                // Если выбран "любой фотограф", находим свободного фотографа
                let photographer_id = if session.photographer_id.is_none() {
                    match find_available_photographer(&pool, session.service_id.unwrap(), booking_start).await {
                        Ok(Some(photographer)) => photographer.id,
                        Ok(None) => {
                            bot.send_message(chat_id, "К сожалению, на выбранное время нет свободных фотографов. Пожалуйста, выберите другое время.").await?;
                            return Ok(());
                        },
                        Err(e) => {
                            error!("Error finding available photographer: {}", e);
                            bot.send_message(chat_id, "Произошла ошибка при поиске фотографа. Пожалуйста, попробуйте позже.").await?;
                            return Ok(());
                        }
                    }
                } else {
                    session.photographer_id.unwrap()
                };

                match create_booking(
                    &pool,
                    session.client_id,
                    photographer_id,
                    session.service_id.unwrap(),
                    booking_start,
                    booking_end
                ).await {
                    Ok(_) => {
                        bot.edit_message_text(chat_id, msg.id, "Запись оформлена! Ожидайте подтверждения фотографа.")
                        .await?;
                    }
                    Err(e) => {
                        error!("Error creating booking: {}", e);
                        bot.send_message(chat_id, "Ошибка при создании записи. Попробуйте еще раз.").await?;
                        session.step = UserStep::MainMenu;
                        let buttons: Vec<Vec<KeyboardButton>> = vec![
                            vec![KeyboardButton::new("Выбрать услугу")],
                            vec![KeyboardButton::new("Личный кабинет")],
                        ];
                        let keyboard = KeyboardMarkup::new(buttons).resize_keyboard();
                        bot.send_message(chat_id, "Выбери действие")
                            .reply_markup(ReplyMarkup::Keyboard(keyboard))
                            .await?;
                    }
                }
                session.step = UserStep::MainMenu;
            } else {
                show_services(bot.clone(), chat_id, &pool).await;
            }
        },
            _ if data.starts_with("working_hours:") => {
                let parts: Vec<&str> = data.split(':').collect();
                if parts.len() == 3 {
                    let start_hour = parts[1].parse::<i32>().unwrap();
                    let end_hour = parts[2].parse::<i32>().unwrap();
                    
                    if let Some(date) = session.selected_date {
                        if let Err(e) = save_working_hours(&pool, session.photographer_id.unwrap(), date, start_hour, end_hour).await {
                            error!("Error saving working hours: {}", e);
                            bot.send_message(chat_id, "Произошла ошибка при сохранении рабочих часов").await?;
                        } else {
                            bot.send_message(chat_id, "Рабочие часы успешно сохранены").await?;
                            show_photographer_schedule(bot.clone(), &msg, &pool, session.photographer_id.unwrap()).await?;
                        }
                    }
                }
            },
            "custom_hours" => {
                session.step = UserStep::CustomHours;
                bot.send_message(chat_id, "Введите рабочие часы в формате ЧЧ:ЧЧ-ЧЧ:ЧЧ (например, 9:00-18:00)").await?;
            },
            "edit_schedule" => {
                let today = time::OffsetDateTime::now_utc();
                let calendar = generate_calendar(today.month() as u32, today.year(), &pool, session.photographer_id.unwrap(), UserType::Photographer).await;
                bot.send_message(chat_id, "Выберите дату для редактирования:")
                    .reply_markup(ReplyMarkup::InlineKeyboard(calendar))
                    .await?;
            },
            "add_working_day" => {
                let today = time::OffsetDateTime::now_utc();
                let calendar = generate_calendar(today.month() as u32, today.year(), &pool, session.photographer_id.unwrap(), UserType::Photographer).await;
                bot.send_message(chat_id, "Выберите дату для добавления рабочего дня:")
                    .reply_markup(ReplyMarkup::InlineKeyboard(calendar))
                    .await?;
            },
            _ if data.starts_with("confirm_booking:") => {
                let booking_id = data.split(':').nth(1).unwrap().parse::<i32>().unwrap();
                
                sqlx::query!(
                    "UPDATE bookings SET status = 'confirmed' WHERE id = $1",
                    booking_id
                )
                .execute(&pool)
                .await?;

                // Уведомляем клиента
                if let Some(booking) = sqlx::query!(
                    "SELECT client_id FROM bookings WHERE id = $1",
                    booking_id
                )
                .fetch_optional(&pool)
                .await? {
                    if let Some(client) = sqlx::query!(
                        "SELECT telegram_id FROM clients WHERE id = $1",
                        booking.client_id
                    )
                    .fetch_optional(&pool)
                    .await? {
                        bot.send_message(ChatId(client.telegram_id), "Ваша запись была подтверждена фотографом! 🎉").await?;
                    }
                }

                if let Some(msg) = q.message.clone() {
                    bot.send_message(chat_id, "✅ Запись подтверждена").await?;
                }
            },
            _ if data.starts_with("client_reject_booking:") => {
                let booking_id = data.split(':').nth(1).unwrap().parse::<i32>().unwrap();
                
                sqlx::query!(
                    "UPDATE bookings SET status = 'cancelled' WHERE id = $1",
                    booking_id
                )
                .execute(&pool)
                .await?;

                // Уведомляем фотографа
                if let Some(booking) = sqlx::query!(
                    "SELECT photographer_id, id FROM bookings WHERE id = $1",
                    booking_id
                )
                .fetch_optional(&pool)
                .await? {
                    if let Some(photographer) = sqlx::query!(
                        "SELECT telegram_id FROM photographers WHERE id = $1",
                        booking.photographer_id
                    )
                    .fetch_optional(&pool)
                    .await? {
                        let text = format!("К сожалению, клиент отменил запись №{} к вам 😔", booking.id);
                        bot.send_message(ChatId(photographer.telegram_id.unwrap()), text).await?;
                    }
                }

                if let Some(msg) = q.message.clone() {
                    let text = format!("❌ Запись №{} отменена", booking_id);
                    bot.send_message(chat_id, text).await?;
                }
            },
            _ if data.starts_with("reject_booking:") => {
                let booking_id = data.split(':').nth(1).unwrap().parse::<i32>().unwrap();
                
                sqlx::query!(
                    "UPDATE bookings SET status = 'cancelled' WHERE id = $1",
                    booking_id
                )
                .execute(&pool)
                .await?;

                // Уведомляем клиента
                if let Some(booking) = sqlx::query!(
                    "SELECT client_id FROM bookings WHERE id = $1",
                    booking_id
                )
                .fetch_optional(&pool)
                .await? {
                    if let Some(client) = sqlx::query!(
                        "SELECT telegram_id FROM clients WHERE id = $1",
                        booking.client_id
                    )
                    .fetch_optional(&pool)
                    .await? {
                        bot.send_message(ChatId(client.telegram_id), "К сожалению, фотограф отклонил вашу запись 😔").await?;
                    }
                }

                if let Some(msg) = q.message.clone() {
                    let text = format!("❌ Запись №{} отменена", booking_id);
                    bot.send_message(chat_id, text).await?;
                }
            },
            _ if data.starts_with("page_upcoming:") => {
                let page = data.split(':').nth(1).unwrap().parse::<usize>().unwrap();
                if let Some(photographer_id) = session.photographer_id {
                    let bookings = sqlx::query_as!(
                        BookingInfo,
                        r#"
                        SELECT 
                            b.id,
                            b.booking_start,
                            b.booking_end,
                            b.status,
                            c.name as client_name,
                            s.name as service_name,
                            b.client_id,
                            b.photographer_id,
                            b.service_id,
                            c.username as client_phone
                        FROM bookings b 
                        JOIN clients c ON b.client_id = c.id 
                        JOIN services s ON b.service_id = s.id 
                        WHERE b.photographer_id = $1 
                        AND b.booking_start >= CURRENT_TIMESTAMP
                        ORDER BY b.booking_start ASC
                        "#,
                        photographer_id
                    )
                    .fetch_all(&pool)
                    .await?;

                    if bookings.is_empty() {
                        if let Some(msg) = q.message.clone() {
                            bot.edit_message_text(chat_id, msg.id(), "У вас нет предстоящих записей").await?;
                        }
                        return Ok(());
                    }

                    let bookings_per_page = 3;
                    let total_pages = (bookings.len() + bookings_per_page - 1) / bookings_per_page;

                    let start_idx = page * bookings_per_page;
                    let end_idx = std::cmp::min(start_idx + bookings_per_page, bookings.len());
                    let page_bookings = &bookings[start_idx..end_idx];

                    let mut message = String::from("📅 Предстоящие записи:\n\n");
                    let mut keyboard = vec![];

                    for booking in page_bookings {
                        let date_format = format_description!("[day].[month].[year]");
                        let time_format = format_description!("[hour]:[minute]");
                        
                        let date = booking.booking_start.format(&date_format).unwrap();
                        let start_time = booking.booking_start.format(&time_format).unwrap();
                        let end_time = booking.booking_end.format(&time_format).unwrap();
                        
                        let status = match booking.status.as_str() {
                            "new" => "🆕 Новый",
                            "confirmed" => "✅ Подтвержден",
                            "completed" => "✅ Выполнен",
                            "cancelled" => "❌ Отменен",
                            _ => booking.status.as_str()
                        };
                        
                        message.push_str(&format!(
                            "*Запись №{}*\n*Дата:* {}\n*Время:* {} - {}\n*Клиент:* {}\n*Услуга:* {}\n*Статус:* {}\n\n",
                            booking.id,
                            date,
                            start_time,
                            end_time,
                            booking.client_name,
                            booking.service_name,
                            status
                        ));
                    }

                    let mut nav_buttons = vec![];
                    if page > 0 {
                        nav_buttons.push(InlineKeyboardButton::callback("⬅️ Назад", format!("page_upcoming:{}", page - 1)));
                    }
                    nav_buttons.push(InlineKeyboardButton::callback(
                        format!("📄 {}/{}", page + 1, total_pages),
                        "ignore".to_string(),
                    ));
                    if page < total_pages - 1 {
                        nav_buttons.push(InlineKeyboardButton::callback("Вперед ➡️", format!("page_upcoming:{}", page + 1)));
                    }
                    keyboard.push(nav_buttons);

                    let keyboard = InlineKeyboardMarkup::new(keyboard);

                    if let Some(msg) = q.message.clone() {
                        bot.edit_message_text(chat_id, msg.id(), message)
                            .parse_mode(teloxide::types::ParseMode::Markdown)
                            .reply_markup(keyboard)
                            .await?;
                    }
                }
            },
            _ if data.starts_with("client_bookings:") => {
                let page = data.split(':').nth(1).unwrap().parse::<usize>().unwrap();
                show_client_bookings(bot.clone(), chat_id, pool, session.client_id, page, session, msg).await?;
            },
            _ if data.starts_with("all_bookings:") => {
                let page = data.split(':').nth(1).unwrap().parse::<usize>().unwrap();
                if let Some(photographer_id) = session.photographer_id {
                    let bookings = sqlx::query_as!(
                        BookingInfo,
                        r#"
                        SELECT 
                            b.id,
                            b.booking_start,
                            b.booking_end,
                            b.status,
                            c.name as client_name,
                            s.name as service_name,
                            b.client_id,
                            b.photographer_id,
                            b.service_id,
                            c.username as client_phone
                        FROM bookings b 
                        JOIN clients c ON b.client_id = c.id 
                        JOIN services s ON b.service_id = s.id 
                        WHERE b.photographer_id = $1 
                        ORDER BY b.booking_start DESC
                        "#,
                        photographer_id
                    )
                    .fetch_all(&pool)
                    .await?;

                    if bookings.is_empty() {
                        bot.send_message(chat_id, "У вас нет записей")
                            .await?;
                        return Ok(());
                    }

                    let bookings_per_page = 3;
                    let total_pages = (bookings.len() + bookings_per_page - 1) / bookings_per_page;

                    let start_idx = page * bookings_per_page;
                    let end_idx = std::cmp::min(start_idx + bookings_per_page, bookings.len());
                    let page_bookings = &bookings[start_idx..end_idx];

                    let mut message = String::from("📋 Все записи:\n\n");
                    let mut keyboard = vec![];

                    for booking in page_bookings {
                        let date_format = format_description!("[day].[month].[year]");
                        let time_format = format_description!("[hour]:[minute]");
                        
                        let date = booking.booking_start.format(&date_format).unwrap();
                        let start_time = booking.booking_start.format(&time_format).unwrap();
                        let end_time = booking.booking_end.format(&time_format).unwrap();
                        
                        let status = match booking.status.as_str() {
                            "new" => "🆕 Новый",
                            "confirmed" => "✅ Подтвержден",
                            "completed" => "✅ Выполнен",
                            "cancelled" => "❌ Отменен",
                            _ => booking.status.as_str()
                        };
                        
                        message.push_str(&format!(
                            "*Запись №{}*\n*Дата:* {}\n*Время:* {} - {}\n*Клиент:* {}\n*Услуга:* {}\n*Статус:* {}\n\n",
                            booking.id,
                            date,
                            start_time,
                            end_time,
                            booking.client_name,
                            booking.service_name,
                            status
                        ));
                        let mut booking_buttons = vec![
                            InlineKeyboardButton::callback(
                                format!("🔢 #{}", booking.id),
                                "ignore".to_string()
                            ),
                        ];
                        if let Some(username) = &booking.client_phone {
                            if !username.is_empty() {
                                let url = format!("https://t.me/{}", username);
                                match Url::parse(&url) {
                                    Ok(parsed_url) => {
                                        booking_buttons.push(InlineKeyboardButton::url(
                                            "📞 Связаться".to_string(),
                                            parsed_url
                                        ));
                                    },
                                    Err(e) => {
                                        println!("Error parsing URL for username {}: {}", username, e);
                                    }
                                }
                            }
                        }
                        keyboard.push(booking_buttons);
                    }

                    // Add navigation buttons
                    let mut nav_buttons = vec![];
                    if page > 0 {
                        nav_buttons.push(InlineKeyboardButton::callback("⬅️ Назад", format!("all_bookings:{}", page - 1)));
                    }
                    nav_buttons.push(InlineKeyboardButton::callback(
                        format!("📄 {}/{}", page + 1, total_pages),
                        "ignore".to_string(),
                    ));
                    if page < total_pages - 1 {
                        nav_buttons.push(InlineKeyboardButton::callback("Вперед ➡️", format!("all_bookings:{}", page + 1)));
                    }
                    keyboard.push(nav_buttons);

                    let keyboard = InlineKeyboardMarkup::new(keyboard);

                    // Update the existing message
                    if let Some(msg) = q.message.clone() {
                        bot.edit_message_text(chat_id, msg.id(), message)
                            .parse_mode(teloxide::types::ParseMode::Markdown)
                            .reply_markup(keyboard)
                            .await?;
                    }
                }
            },
            _ if data == "back_to_services" => {
                if let Some(msg) = q.message.clone() {
                    let services = get_services(&pool).await;
                    let mut keyboard: Vec<Vec<InlineKeyboardButton>> = Vec::new();
                    
                    // Добавляем кнопки для каждой услуги
                    for service in &services {
                        keyboard.push(vec![
                            InlineKeyboardButton::callback(
                                format!("ℹ️ {}", service.name),
                                format!("service_info:{}", service.id)
                            ),
                        ]);
                    }

                    let keyboard = InlineKeyboardMarkup::new(keyboard);

                    bot.edit_message_text(chat_id, msg.id(), "Выбери услугу 📸\n\n")
                        .reply_markup(keyboard)
                        .await?;
                }
            },
            _ if data == "back_to_photographers" => {
                if let Some(service_id) = session.service_id {
                    show_photographers_for_service(bot.clone(), chat_id, &pool, service_id, msg.clone()).await;
                }
            },
            _ if data == "back_to_calendar" => {
                if let (Some(photographer_id), Some(service_id)) = (session.photographer_id, session.service_id) {
    let today_month = chrono::Utc::now().month();
                    let today_year = chrono::Utc::now().year();
                    let key = generate_calendar(today_month, today_year, &pool, photographer_id, UserType::Client).await;
                    bot.send_message(chat_id, "Выбери дату:")
                        .reply_markup(ReplyMarkup::InlineKeyboard(key))
                        .await?;
                }
            },
            _ if data.starts_with("service_info:") => {
                let service_id = data.split(':').nth(1).unwrap().parse::<i32>().unwrap();
                
                // Получаем информацию об услуге
                if let Some(service) = sqlx::query_as::<_, Service>(
                    "SELECT * FROM services WHERE id = $1"
                )
                .bind(service_id)
                .fetch_optional(&pool)
                .await? {
                    let message = format!(
                        "*Информация об услуге*\n\n\
                        🎯 *Название:* {}\n\
                        💰 *Стоимость:* {} руб.\n\
                        ⏱ *Длительность:* {} мин.\n\
                        📝 *Описание:* {}\n\n\
                        Выберите действие:",
                        service.name,
                        service.cost,
                        service.duration,
                        service.comment.unwrap_or_else(|| "Нет описания".to_string())
                    );

                    let keyboard = InlineKeyboardMarkup::new(vec![
                        vec![InlineKeyboardButton::callback(
                            "Выбрать эту услугу".to_string(),
                            format!("service:{}", service.id)
                        )],
                        vec![InlineKeyboardButton::callback(
                            "⟵ Назад к списку услуг".to_string(),
                            "back_to_services".to_string()
                        )],
                    ]);

                    if let Some(msg) = q.message.clone() {
                        bot.edit_message_text(chat_id, msg.id(), message)
                            .parse_mode(teloxide::types::ParseMode::Markdown)
                            .reply_markup(keyboard)
                            .await?;
                    }
                }
            },
            _ if data == "new_bookings" => {
                if let Some(photographer_id) = session.photographer_id {
                    let bookings = sqlx::query_as!(
                        BookingInfo,
                        r#"
                        SELECT 
                            b.id,
                            b.booking_start,
                            b.booking_end,
                            b.status,
                            c.name as client_name,
                            s.name as service_name,
                            b.client_id,
                            b.photographer_id,
                            b.service_id,
                            c.username as client_phone
                        FROM bookings b 
                        JOIN clients c ON b.client_id = c.id 
                        JOIN services s ON b.service_id = s.id 
                        WHERE b.photographer_id = $1 
                        AND b.status = 'new'
                        ORDER BY b.booking_start ASC
                        "#,
                        photographer_id
                    )
                    .fetch_all(&pool)
                    .await?;

                    println!("Found {} bookings", bookings.len());
                    for booking in &bookings {
                        println!("Booking {} - Client username: {:?}", booking.id, booking.client_phone);
                    }

                    if bookings.is_empty() {
                        if let Some(msg) = q.message.clone() {
                            bot.edit_message_text(chat_id, msg.id(), "У вас нет новых записей").await?;
                        }
                        return Ok(());
                    }

                    let bookings_per_page = 3;
                    let total_pages = (bookings.len() + bookings_per_page - 1) / bookings_per_page;
                    let current_page = 0;

                    let start_idx = current_page * bookings_per_page;
                    let end_idx = std::cmp::min(start_idx + bookings_per_page, bookings.len());
                    let page_bookings = &bookings[start_idx..end_idx];

                    let mut message = String::from("🆕 Новые записи:\n\n");
                    let mut keyboard = vec![];

                    for booking in page_bookings {
                        let date_format = format_description!("[day].[month].[year]");
                        let time_format = format_description!("[hour]:[minute]");
                        
                        let date = booking.booking_start.format(&date_format).unwrap();
                        let start_time = booking.booking_start.format(&time_format).unwrap();
                        let end_time = booking.booking_end.format(&time_format).unwrap();
                        
                        message.push_str(&format!(
                            "*Запись №{}*\n*Дата:* {}\n*Время:* {} - {}\n*Клиент:* {}\n*Услуга:* {}\n\n",
                            booking.id,
                            date,
                            start_time,
                            end_time,
                            booking.client_name,
                            booking.service_name
                        ));

                        let mut booking_buttons = vec![
                            InlineKeyboardButton::callback(
                                format!("🔢 #{}", booking.id),
                                "ignore".to_string()
                            ),
                        ];

                        // Добавляем кнопки в зависимости от статуса записи
                        if booking.status == "new" {
                            booking_buttons.push(InlineKeyboardButton::callback(
                                "✅ Подтвердить".to_string(),
                                format!("confirm_booking:{}", booking.id)
                            ));
                            booking_buttons.push(InlineKeyboardButton::callback(
                                "❌ Отменить".to_string(),
                                format!("reject_booking:{}", booking.id)
                            ));
                        }

                        if let Some(username) = &booking.client_phone {
                            if !username.is_empty() {
                                let url = format!("https://t.me/{}", username);
                                match Url::parse(&url) {
                                    Ok(parsed_url) => {
                                        booking_buttons.push(InlineKeyboardButton::url(
                                            "📞 Связаться".to_string(),
                                            parsed_url
                                        ));
                                    },
        Err(e) => {
                                        println!("Error parsing URL for username {}: {}", username, e);
                                    }
                                }
                            }
                        }

                        keyboard.push(booking_buttons);
                    }

                    if bookings.len() > bookings_per_page {
                        let mut nav_buttons = vec![];
                        if current_page > 0 {
                            nav_buttons.push(InlineKeyboardButton::callback("⬅️ Назад", format!("page_new:{}", current_page - 1)));
                        }
                        nav_buttons.push(InlineKeyboardButton::callback(
                            format!("📄 {}/{}", current_page + 1, total_pages),
                            "ignore".to_string(),
                        ));
                        if current_page < total_pages - 1 {
                            nav_buttons.push(InlineKeyboardButton::callback("Вперед ➡️", format!("page_new:{}", current_page + 1)));
                        }
                        keyboard.push(nav_buttons);
                    }

                    let keyboard = InlineKeyboardMarkup::new(keyboard);
                    
                    if let Some(msg) = q.message.clone() {
                        bot.edit_message_text(chat_id, msg.id(), message)
                            .parse_mode(teloxide::types::ParseMode::Markdown)
                            .reply_markup(keyboard)
                            .await?;
                    }
                }
            },
            _ if data.starts_with("page_new:") => {
                let page = data.split(':').nth(1).unwrap().parse::<usize>().unwrap();
                if let Some(photographer_id) = session.photographer_id {
                    let bookings = sqlx::query_as!(
                        BookingInfo,
                        r#"
                        SELECT 
                            b.id,
                            b.booking_start,
                            b.booking_end,
                            b.status,
                            c.name as client_name,
                            s.name as service_name,
                            b.client_id,
                            b.photographer_id,
                            b.service_id,
                            c.username as client_phone
                        FROM bookings b 
                        JOIN clients c ON b.client_id = c.id 
                        JOIN services s ON b.service_id = s.id 
                        WHERE b.photographer_id = $1 
                        AND b.status = 'new'
                        ORDER BY b.booking_start ASC
                        "#,
                        photographer_id
                    )
                    .fetch_all(&pool)
                    .await?;

                    if bookings.is_empty() {
                        if let Some(msg) = q.message.clone() {
                            bot.edit_message_text(chat_id, msg.id(), "У вас нет новых записей").await?;
                        }
                        return Ok(());
                    }

                    let bookings_per_page = 3;
                    let total_pages = (bookings.len() + bookings_per_page - 1) / bookings_per_page;

                    let start_idx = page * bookings_per_page;
                    let end_idx = std::cmp::min(start_idx + bookings_per_page, bookings.len());
                    let page_bookings = &bookings[start_idx..end_idx];

                    let mut message = String::from("🆕 Новые записи:\n\n");
                    let mut keyboard = vec![];

                    for booking in page_bookings {
                        let date_format = format_description!("[day].[month].[year]");
                        let time_format = format_description!("[hour]:[minute]");
                        
                        let date = booking.booking_start.format(&date_format).unwrap();
                        let start_time = booking.booking_start.format(&time_format).unwrap();
                        let end_time = booking.booking_end.format(&time_format).unwrap();
                        
                        message.push_str(&format!(
                            "*Запись №{}*\n*Дата:* {}\n*Время:* {} - {}\n*Клиент:* {}\n*Услуга:* {}\n\n",
                            booking.id,
                            date,
                            start_time,
                            end_time,
                            booking.client_name,
                            booking.service_name
                        ));

                        let mut booking_buttons = vec![
                            InlineKeyboardButton::callback(
                                format!("🔢 #{}", booking.id),
                                "ignore".to_string()
                            ),
                        ];

                        // Добавляем кнопки в зависимости от статуса записи
                        if booking.status == "new" {
                            booking_buttons.push(InlineKeyboardButton::callback(
                                "✅ Подтвердить".to_string(),
                                format!("confirm_booking:{}", booking.id)
                            ));
                            booking_buttons.push(InlineKeyboardButton::callback(
                                "❌ Отменить".to_string(),
                                format!("reject_booking:{}", booking.id)
                            ));
                        }

                        if let Some(username) = &booking.client_phone {
                            if !username.is_empty() {
                                let url = format!("https://t.me/{}", username);
                                match Url::parse(&url) {
                                    Ok(parsed_url) => {
                                        booking_buttons.push(InlineKeyboardButton::url(
                                            "📞 Связаться".to_string(),
                                            parsed_url
                                        ));
                                    },
                                    Err(e) => {
                                        println!("Error parsing URL for username {}: {}", username, e);
                                    }
                                }
                            }
                        }

                        keyboard.push(booking_buttons);
                    }

                    let mut nav_buttons = vec![];
                    if page > 0 {
                        nav_buttons.push(InlineKeyboardButton::callback("⬅️ Назад", format!("page_new:{}", page - 1)));
                    }
                    nav_buttons.push(InlineKeyboardButton::callback(
                        format!("📄 {}/{}", page + 1, total_pages),
                        "ignore".to_string(),
                    ));
                    if page < total_pages - 1 {
                        nav_buttons.push(InlineKeyboardButton::callback("Вперед ➡️", format!("page_new:{}", page + 1)));
                    }
                    keyboard.push(nav_buttons);

                    let keyboard = InlineKeyboardMarkup::new(keyboard);

                    if let Some(msg) = q.message.clone() {
                        bot.edit_message_text(chat_id, msg.id(), message)
                            .parse_mode(teloxide::types::ParseMode::Markdown)
                            .reply_markup(keyboard)
                            .await?;
                    }
                }
            },
            _ if data.starts_with("page_all:") => {
                let page = data.split(':').nth(1).unwrap().parse::<usize>().unwrap();
                if let Some(photographer_id) = session.photographer_id {
                    let bookings = sqlx::query_as!(
                        BookingInfo,
                        r#"
                        SELECT 
                            b.id,
                            b.booking_start,
                            b.booking_end,
                            b.status,
                            c.name as client_name,
                            s.name as service_name,
                            b.client_id,
                            b.photographer_id,
                            b.service_id,
                            c.username as client_phone
                        FROM bookings b 
                        JOIN clients c ON b.client_id = c.id 
                        JOIN services s ON b.service_id = s.id 
                        WHERE b.photographer_id = $1 
                        ORDER BY b.booking_start DESC
                        "#,
                        photographer_id
                    )
                    .fetch_all(&pool)
                    .await?;

                    if bookings.is_empty() {
                        bot.send_message(chat_id, "У вас нет записей")
                            .await?;
                        return Ok(());
                    }

                    let bookings_per_page = 3;
                    let total_pages = (bookings.len() + bookings_per_page - 1) / bookings_per_page;

                    let start_idx = page * bookings_per_page;
                    let end_idx = std::cmp::min(start_idx + bookings_per_page, bookings.len());
                    let page_bookings = &bookings[start_idx..end_idx];

                    let mut message = String::from("📋 Все записи:\n\n");
                    let mut keyboard = vec![];

                    for booking in page_bookings {
                        let date_format = format_description!("[day].[month].[year]");
                        let time_format = format_description!("[hour]:[minute]");
                        
                        let date = booking.booking_start.format(&date_format).unwrap();
                        let start_time = booking.booking_start.format(&time_format).unwrap();
                        let end_time = booking.booking_end.format(&time_format).unwrap();
                        
                        let status = match booking.status.as_str() {
                            "new" => "🆕 Новый",
                            "confirmed" => "✅ Подтвержден",
                            "completed" => "✅ Выполнен",
                            "cancelled" => "❌ Отменен",
                            _ => booking.status.as_str()
                        };
                        
                        message.push_str(&format!(
                            "*Запись №{}*\n*Дата:* {}\n*Время:* {} - {}\n*Клиент:* {}\n*Услуга:* {}\n*Статус:* {}\n\n",
                            booking.id,
                            date,
                            start_time,
                            end_time,
                            booking.client_name,
                            booking.service_name,
                            status
                        ));
                        let mut booking_buttons = vec![
                            InlineKeyboardButton::callback(
                                format!("🔢 #{}", booking.id),
                                "ignore".to_string()
                            ),
                        ];
                        if let Some(username) = &booking.client_phone {
                            if !username.is_empty() {
                                let url = format!("https://t.me/{}", username);
                                match Url::parse(&url) {
                                    Ok(parsed_url) => {
                                        booking_buttons.push(InlineKeyboardButton::url(
                                            "📞 Связаться".to_string(),
                                            parsed_url
                                        ));
                                    },
                                    Err(e) => {
                                        println!("Error parsing URL for username {}: {}", username, e);
                                    }
                                }
                            }
                        }
                        keyboard.push(booking_buttons);
                    }

                    let mut nav_buttons = vec![];
                    if page > 0 {
                        nav_buttons.push(InlineKeyboardButton::callback("⬅️ Назад", format!("page_all:{}", page - 1)));
                    }
                    nav_buttons.push(InlineKeyboardButton::callback(
                        format!("📄 {}/{}", page + 1, total_pages),
                        "ignore".to_string(),
                    ));
                    if page < total_pages - 1 {
                        nav_buttons.push(InlineKeyboardButton::callback("Вперед ➡️", format!("page_all:{}", page + 1)));
                    }
                    keyboard.push(nav_buttons);

                    let keyboard = InlineKeyboardMarkup::new(keyboard);

                    if let Some(msg) = q.message.clone() {
                        bot.edit_message_text(chat_id, msg.id(), message)
                            .parse_mode(teloxide::types::ParseMode::Markdown)
                            .reply_markup(keyboard)
                            .await?;
                    }
                }
            },
            _ if data.starts_with("revoke_consent:") => {
                let action = data.split(':').nth(1).unwrap();
                match action {
                    "confirm" => {
                        // Получаем данные клиента перед архивацией
                        if let Some(client) = sqlx::query!(
                            "SELECT telegram_id, name, username FROM clients WHERE telegram_id = $1",
                            chat_id.0 as i32
                        )
                        .fetch_optional(&pool)
                        .await? {
                            // Перемещаем клиента в архив
                            sqlx::query!(
                                "INSERT INTO archived_clients (telegram_id, name, username)
                                 VALUES ($1, $2, $3)",
                                client.telegram_id,
                                client.name,
                                client.username
                            )
                            .execute(&pool)
                            .await?;

                            // Удаляем клиента из основной таблицы
                            sqlx::query!(
                                "DELETE FROM clients WHERE telegram_id = $1",
                                chat_id.0 as i32
                            )
                            .execute(&pool)
                            .await?;
                            
                            // Сбрасываем сессию
                            session.step = UserStep::Start;
                            session.client_id = -1;
                            session.agreement = false;
                            
                            bot.send_message(chat_id, "Ваше согласие отозвано, и аккаунт перемещен в архив. Для использования бота необходимо зарегистрироваться заново.").await?;
                        }
                    },
                    "cancel" => {
                        if let Some(msg) = q.message.clone() {
                            bot.edit_message_text(chat_id, msg.id(), "Отмена отзыва согласия").await?;
                        }
                    },
                    _ => {}
                }
            },
            _ if data.starts_with("complete_booking:") => {
                let booking_id = data.split(':').nth(1).unwrap().parse::<i32>().unwrap();
                
                sqlx::query!(
                    "UPDATE bookings SET status = 'выполнено' WHERE id = $1",
                    booking_id
                )
                .execute(&pool)
                .await?;

                // Уведомляем клиента
                if let Some(booking) = sqlx::query!(
                    "SELECT client_id FROM bookings WHERE id = $1",
                    booking_id
                )
                .fetch_optional(&pool)
                .await? {
                    if let Some(client) = sqlx::query!(
                        "SELECT telegram_id FROM clients WHERE id = $1",
                        booking.client_id
                    )
                    .fetch_optional(&pool)
                    .await? {
                        bot.send_message(ChatId(client.telegram_id), "Ваша запись была отмечена как завершенная! 🎉").await?;
                    }
                }

                if let Some(msg) = q.message.clone() {
                    let text = format!("✅ Запись №{} отмечена как завершенная",booking_id);
                    bot.send_message(chat_id, text).await?;
                }
            },
            _ => {}
        }
        bot.answer_callback_query(q.id).await?;
    }
    Ok(())
}

async fn get_free_slots(
    pool: &PgPool,
    photographer_id: i32,
    service_id: i32,
    date: PrimitiveDateTime,
) -> Result<Vec<String>, sqlx::Error> {
    // 1. Получаем длительность услуги в минутах и конвертируем в часы
    let duration_minutes: i32 = sqlx::query_scalar!(
        "SELECT duration FROM services WHERE id = $1",
        service_id
    )
    .fetch_one(pool)
    .await?;

    let duration_hours = (duration_minutes as f64 / 60.0).ceil() as i32;
    println!("Duration in minutes: {}, Duration in hours: {}", duration_minutes, duration_hours);

    // 2. Получаем рабочие часы фотографа на эту дату
    let working_hours = sqlx::query!(
        "SELECT start_hour, end_hour FROM working_hours
         WHERE photographer_id = $1 AND date = $2",
        photographer_id,
        date.date()
    )
    .fetch_optional(pool)
    .await?;

    println!("Working hours: {:?}", working_hours);

    let (start_hour, end_hour) = match working_hours {
        Some(hours) => (hours.start_hour, hours.end_hour),
        None => return Ok(vec![]), // Если нет рабочих часов, возвращаем пустой список
    };

    println!("Start hour: {}, End hour: {}", start_hour, end_hour);

    // 3. Получаем все бронирования на эту дату
    let date_offset = date.assume_utc();
    let bookings = sqlx::query!(
        "SELECT booking_start, booking_end FROM bookings
         WHERE photographer_id = $1
         AND DATE(booking_start) = DATE($2)
         AND status != 'cancelled'",
        photographer_id,
        date_offset
    )
    .fetch_all(pool)
    .await?;

    println!("Bookings count: {}", bookings.len());

    // 4. Строим часовые слоты в пределах рабочих часов
    let mut free_slots = vec![];
    
    
    // Начинаем с начала рабочего дня
    let mut current_hour = start_hour;
    
    // Продолжаем, пока текущий час + длительность услуги не превысит конец рабочего дня
    while current_hour + duration_hours <= end_hour {
        let slot_start = PrimitiveDateTime::new(date.date(), time!(0:00) + time::Duration::hours(current_hour as i64));
        let slot_end = slot_start + time::Duration::hours(duration_hours as i64);

        // Проверяем, пересекается ли слот с существующими бронированиями
        let is_slot_free = !bookings.iter().any(|b| {
            let booking_start = b.booking_start;
            let booking_end = b.booking_end;
            
            // Проверяем пересечение временных интервалов
            let slot_start_time = slot_start.time();
            let slot_end_time = slot_end.time();
            let booking_start_time = booking_start.time();
            let booking_end_time = booking_end.time();
            
            let overlaps = (slot_start_time < booking_end_time) && (slot_end_time > booking_start_time);
            println!("Slot {}:00-{}:00 overlaps with booking {}:00-{}:00: {}", 
                current_hour, current_hour + duration_hours,
                booking_start_time.hour(), booking_end_time.hour(),
                overlaps);
            overlaps
        });

        if is_slot_free {
            println!("Adding free slot: {}:00-{}:00", current_hour, current_hour + duration_hours);
            free_slots.push(format!("{:02}:00-{:02}:00", current_hour, current_hour + duration_hours));
        }

        current_hour += 1;
    }

    println!("Total free slots: {}", free_slots.len());
    Ok(free_slots)
}

// Функции для работы с БД
async fn get_services(pool: &PgPool) -> Vec<Service> {
    sqlx::query_as::<_, Service>("SELECT * FROM services")
        .fetch_all(pool)
        .await
        .unwrap()
}

async fn show_services(bot: Bot, chat_id: ChatId, pool: &PgPool) {
    let services = get_services(pool).await;

    let mut keyboard: Vec<Vec<InlineKeyboardButton>> = Vec::new();
    
    // Добавляем кнопки для каждой услуги
    for service in &services {
        keyboard.push(vec![
            InlineKeyboardButton::callback(
                format!("ℹ️ {}", service.name),
                format!("service_info:{}", service.id)
            ),
        ]);
    }

    let keyboard = InlineKeyboardMarkup::new(keyboard);

    bot.send_message(chat_id, "Выбери услугу 📸\n\nНажми ℹ️ для просмотра подробной информации об услуге")
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

async fn show_photographers_for_service(bot: Bot, chat_id: ChatId, pool: &PgPool, service_id: i32, msg: Message) {
    let photographers = get_photographers_by_service(pool, service_id).await;

    if photographers.is_empty() {
        bot.send_message(chat_id, "Нет доступных фотографов для этой услуги 😢")
            .await
            .unwrap();
        return;
    }

    let mut keyboard: Vec<Vec<InlineKeyboardButton>> = Vec::new();
    
    // Add "Any photographer" button
    keyboard.push(vec![InlineKeyboardButton::callback(
        "📸 Любой фотограф".to_string(),
        "photographer:any"
    )]);
    
    // Add photographer buttons
    for p in &photographers {
        keyboard.push(vec![InlineKeyboardButton::callback(
            p.name.clone(),
            format!("photographer:{}", p.id)
        ),
        InlineKeyboardButton::callback(
            "ℹ️ Подробнее".to_string(),
            format!("photographer_info:{}", p.id)
        ),
        ]);
    }
    
    // Add back button
    keyboard.push(vec![InlineKeyboardButton::callback(
        "⟵ Назад к услугам".to_string(),
        "back_to_services".to_string()
    )]);

    let keyboard = InlineKeyboardMarkup::new(keyboard);

    bot.edit_message_text(chat_id, msg.id, "Выбери фотографа 📷\n\nИли выбери 'Любой фотограф' для автоматического назначения")
                                    .parse_mode(teloxide::types::ParseMode::Markdown)
                                    .reply_markup(keyboard)
                                    .await;
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

pub async fn generate_calendar(month: u32, year: i32, pool: &PgPool, photographer_id: i32, user_type: UserType) -> InlineKeyboardMarkup {
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
            let naive_date = NaiveDate::from_ymd_opt(year, month, day).unwrap();
            let today = Local::now().date_naive();
            let date = Date::from_calendar_date(year, Month::try_from(month as u8).unwrap(), day as u8).unwrap();
            
            if naive_date < today {
                // Для дат в прошлом добавляем неактивную кнопку
                row.push(InlineKeyboardButton::callback(
                    format!("❌ {}", day),
                    "ignore".to_string(),
                ));
            } else {
                // Проверяем, является ли день рабочим
                let is_working_day = if photographer_id == -1 {
                    // Для "любого фотографа" проверяем наличие хотя бы одного фотографа с рабочими часами
                    check_any_photographer_available(pool, date).await
                } else {
                    if let Some((start_hour, end_hour)) = get_working_hours(pool, photographer_id, date).await {
                        start_hour > 0 && end_hour > 0
                    } else {
                        false
                    }
                };

                let callback = format!("calendar:select:{}", naive_date);
                let button_text = if is_working_day {
                    format!("{:2}", day) // Просто число для рабочих дней
                } else {
                    format!("❌ {:2}", day) // Крестик для нерабочих дней
                };
                row.push(InlineKeyboardButton::callback(button_text, callback));
            }

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

    // 4. Переключатели месяцев
    keyboard.push(vec![
        InlineKeyboardButton::callback("< Месяц", format!("calendar:prev_month:{}:{}", month, year)),
        InlineKeyboardButton::callback("Месяц >", format!("calendar:next_month:{}:{}", month, year)),
    ]);

    // 5. Back button - только для клиентов
    if user_type == UserType::Client {
        keyboard.push(vec![InlineKeyboardButton::callback(
            "⟵ Назад к фотографам".to_string(),
            "back_to_photographers".to_string()
        )]);
    }

    InlineKeyboardMarkup::new(keyboard)
}

// Обновляем функцию проверки доступности любого фотографа
async fn check_any_photographer_available(pool: &PgPool, date: Date) -> bool {
    // Проверяем, есть ли хотя бы один фотограф с рабочими часами на эту дату
    let result = sqlx::query!(
        "SELECT EXISTS (
            SELECT 1 FROM working_hours wh
            JOIN photographers p ON wh.photographer_id = p.id
            JOIN photographer_services ps ON p.id = ps.photographer_id
            WHERE wh.date = $1
            AND wh.start_hour > 0
            AND wh.end_hour > 0
        ) as exists",
        date
    )
    .fetch_one(pool)
    .await;

    match result {
        Ok(row) => row.exists.unwrap_or(false),
        Err(_) => false
    }
}

// Обновляем функцию получения доступных слотов для любого фотографа
async fn get_available_photographers(pool: &PgPool, service_id: i32, date: PrimitiveDateTime) -> Result<Vec<String>, sqlx::Error> {
    // Получаем всех фотографов, которые предоставляют данную услугу и имеют рабочие часы на эту дату
    let photographers = sqlx::query!(
        "SELECT DISTINCT p.id 
         FROM photographers p
         JOIN photographer_services ps ON p.id = ps.photographer_id
         JOIN working_hours wh ON p.id = wh.photographer_id
         WHERE ps.service_id = $1
         AND wh.date = $2
         AND wh.start_hour > 0
         AND wh.end_hour > 0",
        service_id,
        date.date()
    )
    .fetch_all(pool)
    .await?;

    let mut all_slots = Vec::new();
    
    // Для каждого фотографа получаем свободные слоты
    for photographer in photographers {
        if let Ok(slots) = get_free_slots(pool, photographer.id, service_id, date).await {
            all_slots.extend(slots);
        }
    }
    
    // Удаляем дубликаты слотов и сортируем их
    all_slots.sort();
    all_slots.dedup();
    
    println!("Found {} available slots for any photographer", all_slots.len());
    for slot in &all_slots {
        println!("Available slot: {}", slot);
    }
    
    Ok(all_slots)
}

async fn create_booking(pool: &PgPool, client_id: i32, photographer_id: i32, service_id: i32, booking_start: PrimitiveDateTime, booking_end: PrimitiveDateTime) -> Result<i32, sqlx::Error> {
    let booking_id = sqlx::query_scalar!(
        "INSERT INTO bookings (client_id, photographer_id, service_id, booking_start, booking_end, status)
         VALUES ($1, $2, $3, $4, $5, 'new')
         RETURNING id",
        client_id,
        photographer_id,
        service_id,
        booking_start,
        booking_end
    )
    .fetch_one(pool)
    .await?;

    // Отправляем уведомление фотографу
    if let Some(photographer) = sqlx::query!(
        "SELECT telegram_id FROM photographers WHERE id = $1",
        photographer_id
    )
    .fetch_optional(pool)
    .await?
    {
        if let Some(telegram_id) = photographer.telegram_id {
            let bot = Bot::from_env();
            let booking_info = sqlx::query_as!(
                BookingInfo,
                r#"
                SELECT 
                    b.id,
                    b.booking_start,
                    b.booking_end,
                    b.status,
                    c.name as client_name,
                    s.name as service_name,
                    b.client_id,
                    b.photographer_id,
                    b.service_id,
                    CAST(c.telegram_id AS TEXT) as client_phone
                FROM bookings b
                JOIN clients c ON b.client_id = c.id
                JOIN services s ON b.service_id = s.id
                WHERE b.id = $1
                "#,
                booking_id
            )
            .fetch_one(pool)
            .await?;

            let format = format_description!("[day].[month].[year] [hour]:[minute]");
            let start_time = booking_info.booking_start.format(&format).unwrap();
            let end_time = booking_info.booking_end.format(&format).unwrap();

            let message = format!(
                "🆕 *Новая запись!*\n\n\
                👤 *Клиент:* {}\n\
                📸 *Услуга:* {}\n\
                📅 *Дата и время:* {} - {}\n\n\
                Для подтверждения записи используйте кнопки в разделе 'Мои записи'",
                booking_info.client_name,
                booking_info.service_name,
                start_time,
                end_time
            );
            println!("Sending notification to photographer with telegram_id: {}", telegram_id);

            // Отправляем уведомление фотографу, используя абсолютное значение telegram_id
            if let Err(e) = bot.send_message(ChatId(telegram_id.abs() as i64), message)
                .parse_mode(teloxide::types::ParseMode::Markdown)
                .await {
                    error!("Failed to send notification to photographer: {}", e);
                // Продолжаем выполнение, даже если не удалось отправить уведомление
            }
        }
    }

    Ok(booking_id)
}

async fn check_client (pool: &PgPool, telegram_id: i64) -> Option<Client> {
    sqlx::query_as::<_, Client>("SELECT * FROM clients WHERE telegram_id = $1")
        .bind(telegram_id as i32)
        .fetch_optional(pool)
        .await
        .unwrap()
}

async fn check_photographer(pool: &PgPool, telegram_id: i64) -> Option<Photographer> {
    sqlx::query_as::<_, Photographer>("SELECT * FROM photographers WHERE telegram_id = $1")
        .bind(telegram_id as i64)
        .fetch_optional(pool)
        .await
        .unwrap()
}

async fn show_photographer_menu(bot: Bot, chat_id: ChatId) -> Result<(), Box<dyn Error + Send + Sync>> {
    let buttons: Vec<Vec<KeyboardButton>> = vec![
        vec![KeyboardButton::new("Моё расписание")],
        vec![KeyboardButton::new("Мои записи")],
        vec![KeyboardButton::new("Изменить портфолио")],
        vec![KeyboardButton::new("Изменить свое описание")],
    ];

    let keyboard = KeyboardMarkup::new(buttons).resize_keyboard();
    bot.send_message(chat_id, "Выбери действие")
        .reply_markup(ReplyMarkup::Keyboard(keyboard))
        .await?;

    Ok(())
}

async fn show_photographer_schedule(bot: Bot, msg: &Message, pool: &PgPool, photographer_id: i32) -> Result<(), Box<dyn Error + Send + Sync>> {
    let today = time::OffsetDateTime::now_utc();
    let calendar = generate_calendar(today.month() as u32, today.year(), &pool, photographer_id, UserType::Photographer).await;
    
    if let Some(reply_to) = msg.reply_to_message() {
        bot.edit_message_text(msg.chat.id, reply_to.id, "Выберите дату для просмотра или редактирования расписания:")
            .reply_markup(calendar)
            .await?;
    } else {
        bot.send_message(msg.chat.id, "Выберите дату для просмотра или редактирования расписания:")
            .reply_markup(ReplyMarkup::InlineKeyboard(calendar))
            .await?;
    }

    Ok(())
}

async fn show_photographer_bookings(bot: Bot, chat_id: ChatId, pool: &PgPool, photographer_id: i32) -> Result<(), Box<dyn Error + Send + Sync>> {
    let keyboard = InlineKeyboardMarkup::new(vec![
        vec![InlineKeyboardButton::callback("🆕 Новые записи", "new_bookings")],
        vec![InlineKeyboardButton::callback("📅 Предстоящие записи", "upcoming_bookings")],
        vec![InlineKeyboardButton::callback("📋 Все записи", "all_bookings")],
    ]);

    bot.send_message(chat_id, "Выберите тип записей для просмотра:")
        .reply_markup(ReplyMarkup::InlineKeyboard(keyboard))
        .await?;

    Ok(())
}

async fn show_time_slots(bot: Bot, chat_id: ChatId, slots: Vec<String>, message_id: MessageId) -> Result<(), Box<dyn Error + Send + Sync>> {
    let mut keyboard: Vec<Vec<InlineKeyboardButton>> = Vec::new();
    let mut current_row: Vec<InlineKeyboardButton> = Vec::new();
    
    for (index, slot) in slots.iter().enumerate() {
        let button = InlineKeyboardButton::callback(
            slot.clone(),
            format!("time-{}", slot)
        );
        
        current_row.push(button);
        
        if current_row.len() == 2 || index == slots.len() - 1 {
            keyboard.push(current_row);
            current_row = Vec::new();
        }
    }

    keyboard.push(vec![InlineKeyboardButton::callback(
        "⟵ Назад к выбору даты".to_string(),
        "back_to_calendar".to_string()
    )]);

    let markup = InlineKeyboardMarkup::new(keyboard);
    bot.edit_message_text(chat_id, message_id, "Выберите удобное время:")
        .reply_markup(markup)
        .await?;

    Ok(())
}

async fn notify_photographer(bot: &Bot, photographer_id: i32, pool: &PgPool, booking_id: i32) -> Result<(), Box<dyn Error + Send + Sync>> {
    let photographer = sqlx::query!(
        "SELECT telegram_id FROM photographers WHERE id = $1",
        photographer_id
    )
    .fetch_one(pool)
    .await?;

    let booking_info = sqlx::query_as!(
        BookingInfo,
        r#"
        SELECT 
            b.id,
            b.booking_start,
            b.booking_end,
            b.status,
            c.name as client_name,
            s.name as service_name,
            b.client_id,
            b.photographer_id,
            b.service_id,
            CAST(c.telegram_id AS TEXT) as client_phone
        FROM bookings b
        JOIN clients c ON b.client_id = c.id
        JOIN services s ON b.service_id = s.id
        WHERE b.id = $1
        "#,
        booking_id
    )
    .fetch_one(pool)
    .await?;

    let format = format_description!("[day].[month].[year] [hour]:[minute]");
    let start_time = booking_info.booking_start.format(&format).unwrap();
    let end_time = booking_info.booking_end.format(&format).unwrap();

    let message = format!(
        "🆕 *Новая запись!*\n\n\
        👤 *Клиент:* {}\n\
        📸 *Услуга:* {}\n\
        📅 *Дата и время:* {} - {}\n\n\
        Для подтверждения записи используйте кнопки в разделе 'Мои записи'",
        booking_info.client_name,
        booking_info.service_name,
        start_time,
        end_time
    );

    if let Some(telegram_id) = photographer.telegram_id {
        bot.send_message(ChatId(telegram_id as i64), message)
        .parse_mode(teloxide::types::ParseMode::Markdown)
        .await?;
    }

    Ok(())
}

async fn add_working_day(bot: Bot, chat_id: ChatId, pool: &PgPool, photographer_id: i32, date: Date) -> Result<(), Box<dyn Error + Send + Sync>> {
    let mut message = String::new();
    println!("chat_id: {}", chat_id.0);
    
    // Проверяем, есть ли уже рабочие часы на эту дату
    if let Some((start_hour, end_hour)) = get_working_hours(pool, photographer_id, date).await {
        message = format!(
            "Текущие рабочие часы на {}: {}:00-{}:00\n\nВыберите новые рабочие часы:",
            date,
            start_hour,
            end_hour
        );
    } else {
        message = format!("Выберите рабочие часы на {}:", date);
    }

    let keyboard = InlineKeyboardMarkup::new(vec![
        vec![InlineKeyboardButton::callback("8:00-20:00", format!("working_hours:8:20"))],
        vec![InlineKeyboardButton::callback("9:00-19:00", format!("working_hours:9:19"))],
        vec![InlineKeyboardButton::callback("10:00-18:00", format!("working_hours:10:18"))],
        vec![InlineKeyboardButton::callback("Настроить свои часы", "custom_hours")],
    ]);

    bot.send_message(chat_id, message)
        .reply_markup(ReplyMarkup::InlineKeyboard(keyboard))
        .await?;

    Ok(())
}

async fn save_working_hours(pool: &PgPool, photographer_id: i32, date: Date, start_hour: i32, end_hour: i32) -> Result<(), sqlx::Error> {
    sqlx::query!(
        "INSERT INTO working_hours (photographer_id, date, start_hour, end_hour)
         VALUES ($1, $2, $3, $4)
         ON CONFLICT (photographer_id, date) DO UPDATE
         SET start_hour = $3, end_hour = $4",
        photographer_id,
        date,
        start_hour,
        end_hour
    )
    .execute(pool)
    .await?;

    Ok(())
}

async fn get_working_hours(pool: &PgPool, photographer_id: i32, date: Date) -> Option<(i32, i32)> {
    let hours = sqlx::query!(
        "SELECT start_hour, end_hour FROM working_hours
         WHERE photographer_id = $1 AND date = $2",
        photographer_id,
        date
    )
    .fetch_optional(pool)
    .await
    .unwrap();

    hours.map(|h| (h.start_hour, h.end_hour))
}

async fn show_client_bookings(bot: Bot, chat_id: ChatId, pool: PgPool, client_id: i32, page: usize, session: &mut UserSession, msg: Message) -> Result<(), Box<dyn Error + Send + Sync>> {
    let bookings = sqlx::query!(
        r#"
        SELECT b.*, p.name as photographer_name, s.name as service_name
        FROM bookings b
        JOIN photographers p ON b.photographer_id = p.id
        JOIN services s ON b.service_id = s.id
        WHERE b.client_id = $1
        ORDER BY b.booking_start DESC
        "#,
        session.client_id
    )
    .fetch_all(&pool)
    .await?;

    if bookings.is_empty() {
        bot.send_message(chat_id, "У вас пока нет записей")
            .await?;
        return Ok(());
    }

    let bookings_per_page = 3;
    let total_pages = (bookings.len() + bookings_per_page - 1) / bookings_per_page;

    let start_idx = page * bookings_per_page;
    let end_idx = std::cmp::min(start_idx + bookings_per_page, bookings.len());
    let page_bookings = &bookings[start_idx..end_idx];

    let mut message = String::from("📋 История ваших записей:\n\n");
    let mut keyboard = vec![];

    for booking in page_bookings {
        let date_format = format_description!("[day].[month].[year]");
        let time_format = format_description!("[hour]:[minute]");
        
        let date = booking.booking_start.format(&date_format).unwrap();
        let start_time = booking.booking_start.format(&time_format).unwrap();
        let end_time = booking.booking_end.format(&time_format).unwrap();
        
        let status = match booking.status.as_str() {
            "new" => "🆕 Новый",
            "confirmed" => "✅ Подтвержден",
            "completed" => "✅ Выполнен",
            "cancelled" => "❌ Отменен",
            _ => booking.status.as_str()
        };
        
        message.push_str(&format!(
            "*Номер записи: {}*\n*Дата:* {}\n*Время:* {} - {}\n*Фотограф: *{}\n*Услуга:* {}\n*Статус:* {}\n\n",
            booking.id,
            date,
            start_time,
            end_time,
            booking.photographer_name,
            booking.service_name,
            status
        ));
        if booking.status == "confirmed" || booking.status == "new" {
            let mut booking_buttons = vec![
                InlineKeyboardButton::callback(
                    format!("🔢 #{}", booking.id),
                    "ignore".to_string()
                ),
                ];

                // Добавляем кнопки в зависимости от статуса записи
                    booking_buttons.push(InlineKeyboardButton::callback(
                        "❌ Отменить".to_string(),
                        format!("client_reject_booking:{}", booking.id)
                    ));
                keyboard.push(booking_buttons);
        }
    }

    // Add navigation buttons
    let mut nav_buttons = vec![];
    if page > 0 {
        nav_buttons.push(InlineKeyboardButton::callback("⬅️ Назад", format!("client_bookings:{}", page - 1)));
    }
    nav_buttons.push(InlineKeyboardButton::callback(
        format!("📄 {}/{}", page + 1, total_pages),
        "ignore".to_string(),
    ));
    if page < total_pages - 1 {
        nav_buttons.push(InlineKeyboardButton::callback("Вперед ➡️", format!("client_bookings:{}", page + 1)));
    }
    keyboard.push(nav_buttons);

    let keyboard = InlineKeyboardMarkup::new(keyboard);

    // Update the existing message
    bot.edit_message_text(chat_id, msg.id, message)
        .parse_mode(teloxide::types::ParseMode::Markdown)
        .reply_markup(keyboard)
        .await?;
    Ok(())
}

async fn find_available_photographer(pool: &PgPool, service_id: i32, date_time: PrimitiveDateTime) -> Result<Option<Photographer>, sqlx::Error> {
    // Получаем всех фотографов, которые предоставляют данную услугу
    let photographers = sqlx::query!(
        "SELECT p.id FROM photographers p
         JOIN photographer_services ps ON p.id = ps.photographer_id
         WHERE ps.service_id = $1",
        service_id
    )
    .fetch_all(pool)
    .await?;

    // Проверяем каждого фотографа на наличие свободных слотов
    for photographer in photographers {
        if let Ok(slots) = get_free_slots(pool, photographer.id, service_id, date_time).await {
            if !slots.is_empty() {
                // Если нашли свободного фотографа, возвращаем его данные
                return Ok(Some(sqlx::query_as::<_, Photographer>(
                    "SELECT * FROM photographers WHERE id = $1"
                )
                .bind(photographer.id)
                .fetch_one(pool)
                .await?));
            }
        }
    }

    Ok(None)
}

// Вспомогательные функции для работы с календарем
fn month_name(month: u32) -> &'static str {
    match month {
        1 => "Январь", 2 => "Февраль", 3 => "Март", 4 => "Апрель",
        5 => "Май", 6 => "Июнь", 7 => "Июль", 8 => "Август",
        9 => "Сентябрь", 10 => "Октябрь", 11 => "Ноябрь", 12 => "Декабрь",
        _ => "",
    }
}

fn month_name_from_month(month: Month) -> &'static str {
    match month {
        Month::January => "января", 
        Month::February => "февраля", 
        Month::March => "марта", 
        Month::April => "апреля",
        Month::May => "мая", 
        Month::June => "июнь", 
        Month::July => "июль", 
        Month::August => "августа",
        Month::September => "сентября", 
        Month::October => "октября", 
        Month::November => "ноября", 
        Month::December => "декабря",
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
