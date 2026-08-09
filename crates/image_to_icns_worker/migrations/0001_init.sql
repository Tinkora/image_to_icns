-- Session records contain metadata only; source images never enter D1.
CREATE TABLE IF NOT EXISTS sessions (
    session_id      TEXT PRIMARY KEY,
    secret_hash     TEXT NOT NULL,
    state           TEXT NOT NULL DEFAULT 'created'
                    CHECK (state IN ('created', 'editing', 'completed', 'cancelled', 'expired', 'failed')),
    source_format   TEXT CHECK (source_format IS NULL OR source_format IN ('png', 'jpeg', 'svg')),
    created_at      TEXT NOT NULL,
    expires_at      TEXT NOT NULL,
    output_byte_len INTEGER CHECK (output_byte_len IS NULL OR output_byte_len > 0),
    representation_count INTEGER CHECK (representation_count IS NULL OR representation_count = 10),
    error_code      TEXT
);

-- Expiration and state indexes support scheduled cleanup.
CREATE INDEX IF NOT EXISTS idx_sessions_expires ON sessions(expires_at);
CREATE INDEX IF NOT EXISTS idx_sessions_state ON sessions(state);

-- Fixed-window counters are isolated by IP and window start time.
CREATE TABLE IF NOT EXISTS rate_limits (
    ip           TEXT NOT NULL,
    window_start INTEGER NOT NULL,
    count        INTEGER NOT NULL DEFAULT 1,
    PRIMARY KEY (ip, window_start)
);

CREATE INDEX IF NOT EXISTS idx_rate_limits_window
    ON rate_limits(window_start);
