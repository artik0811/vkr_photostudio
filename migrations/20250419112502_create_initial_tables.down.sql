DELETE FROM materials;
ALTER SEQUENCE materials_id_seq RESTART WITH 1;

DELETE FROM bookings;
ALTER SEQUENCE bookings_id_seq RESTART WITH 1;

DELETE FROM photographer_services;
ALTER SEQUENCE photographer_services_id_seq RESTART WITH 1;

DELETE FROM services;
ALTER SEQUENCE services_id_seq RESTART WITH 1;

DELETE FROM clients;
ALTER SEQUENCE clients_id_seq RESTART WITH 1;

DELETE FROM photographers;
ALTER SEQUENCE photographers_id_seq RESTART WITH 1;

DROP TABLE IF EXISTS photographer_services;
DROP TABLE IF EXISTS materials;
DROP TABLE IF EXISTS bookings;
DROP TABLE IF EXISTS services;
DROP TABLE IF EXISTS clients;
DROP TABLE IF EXISTS photographers;