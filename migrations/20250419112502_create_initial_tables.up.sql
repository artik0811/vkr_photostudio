-- Add migration script here
CREATE TABLE photographers (
    id SERIAL PRIMARY KEY,
    name TEXT NOT NULL,
    telegram_id BIGINT UNIQUE
);

CREATE TABLE clients (
    id SERIAL PRIMARY KEY,
    telegram_id BIGINT NOT NULL UNIQUE,
    name TEXT NOT NULL
);

CREATE TABLE services (
    id SERIAL PRIMARY KEY,
    name TEXT NOT NULL,
    cost INTEGER NOT NULL,
    duration INTEGER NOT NULL,
    comment TEXT NOT NULL
);

CREATE TABLE bookings (
    id SERIAL PRIMARY KEY,
    client_id INTEGER REFERENCES clients(id),
    photographer_id INTEGER REFERENCES photographers(id),
    service_id INTEGER REFERENCES services(id),
    booking_start TIMESTAMP NOT NULL,
    booking_end TIMESTAMP NOT NULL,
    status TEXT NOT NULL
);

CREATE TABLE materials (
    id SERIAL PRIMARY KEY,
    booking_id INTEGER REFERENCES bookings(id),
    file_url TEXT NOT NULL
);

CREATE TABLE photographer_services (
    id SERIAL PRIMARY KEY,
    photographer_id INTEGER REFERENCES photographers(id),
    service_id INTEGER REFERENCES services(id)
);

CREATE TABLE IF NOT EXISTS working_hours (
    id SERIAL PRIMARY KEY,
    photographer_id INTEGER REFERENCES photographers(id),
    date DATE NOT NULL,
    start_hour INTEGER NOT NULL,
    end_hour INTEGER NOT NULL,
    created_at TIMESTAMP WITH TIME ZONE DEFAULT CURRENT_TIMESTAMP,
    UNIQUE(photographer_id, date)
);