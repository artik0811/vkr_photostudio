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