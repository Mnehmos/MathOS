CREATE TABLE idempotency_results (
    operation TEXT NOT NULL,
    idempotency_key TEXT NOT NULL,
    input_hash TEXT NOT NULL CHECK (length(input_hash) = 64),
    result_json TEXT NOT NULL CHECK (json_valid(result_json)),
    created_at INTEGER NOT NULL,
    PRIMARY KEY (operation, idempotency_key)
) STRICT;
