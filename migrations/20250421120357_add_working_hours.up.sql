CREATE TABLE IF NOT EXISTS working_hours (
    id SERIAL PRIMARY KEY,
    photographer_id INTEGER NOT NULL REFERENCES photographers(id),
    date DATE NOT NULL,
    start_hour INTEGER NOT NULL,
    end_hour INTEGER NOT NULL,
    UNIQUE(photographer_id, date)
); 