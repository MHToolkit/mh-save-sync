-- Durable idempotency receipt for the crash window where PostgreSQL commits a
-- snapshot but the HTTP response never reaches the client. The receipt stores
-- only opaque identifiers and the exact CAS outcome; no plaintext save data or
-- paths are introduced.
CREATE TABLE IF NOT EXISTS snapshot_commit_receipts (
  account_handle BYTEA NOT NULL REFERENCES accounts(account_handle),
  upload_id UUID NOT NULL,
  snapshot_id TEXT NOT NULL,
  device_cert_id BYTEA NOT NULL,
  logical_save_id TEXT NOT NULL,
  manifest_id TEXT NOT NULL,
  parents JSONB NOT NULL,
  required_chunks JSONB NOT NULL,
  outcome TEXT NOT NULL CHECK (outcome IN ('first-snapshot','fast-forward','conflict')),
  outcome_head TEXT NOT NULL,
  conflict_snapshot_id TEXT,
  created_at TIMESTAMPTZ NOT NULL DEFAULT now(),
  PRIMARY KEY (account_handle, snapshot_id),
  UNIQUE (account_handle, upload_id),
  FOREIGN KEY (account_handle, snapshot_id)
    REFERENCES snapshots(account_handle, id) ON DELETE CASCADE,
  FOREIGN KEY (account_handle, logical_save_id)
    REFERENCES logical_saves(account_handle, id),
  FOREIGN KEY (account_handle, device_cert_id)
    REFERENCES devices(account_handle, cert_id)
);

CREATE INDEX IF NOT EXISTS snapshot_commit_receipts_upload_idx
  ON snapshot_commit_receipts(account_handle, upload_id);
