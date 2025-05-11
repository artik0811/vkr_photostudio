CREATE TABLE IF NOT EXISTS photographers (
    id SERIAL PRIMARY KEY,
    name TEXT NOT NULL,
    telegram_id BIGINT UNIQUE,
    portfolio_url TEXT,
    description TEXT
);

CREATE TABLE IF NOT EXISTS clients (
    id SERIAL PRIMARY KEY,
    telegram_id BIGINT NOT NULL UNIQUE,
    name TEXT NOT NULL,
    username TEXT
);

CREATE TABLE IF NOT EXISTS services (
    id SERIAL PRIMARY KEY,
    name TEXT NOT NULL,
    cost INTEGER NOT NULL,
    duration INTEGER NOT NULL,
    comment TEXT NOT NULL
);

CREATE TABLE IF NOT EXISTS bookings (
    id SERIAL PRIMARY KEY,
    client_id INTEGER REFERENCES clients(id),
    photographer_id INTEGER REFERENCES photographers(id),
    service_id INTEGER REFERENCES services(id),
    booking_start TIMESTAMP NOT NULL,
    booking_end TIMESTAMP NOT NULL,
    description TEXT,
    status TEXT NOT NULL
);

CREATE TABLE IF NOT EXISTS photographer_services (
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

CREATE TABLE IF NOT EXISTS archived_clients (
    id SERIAL PRIMARY KEY,
    telegram_id BIGINT NOT NULL,
    name VARCHAR(255) NOT NULL,
    username VARCHAR(255),
    archived_at TIMESTAMP WITH TIME ZONE DEFAULT CURRENT_TIMESTAMP
); 