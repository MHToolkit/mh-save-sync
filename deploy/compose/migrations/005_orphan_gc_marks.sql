CREATE TABLE IF NOT EXISTS orphan_gc_marks (
  account_handle BYTEA NOT NULL,
  object_id TEXT NOT NULL,
  storage_key TEXT NOT NULL,
  marked_at TIMESTAMPTZ NOT NULL DEFAULT now(),
  lease_token UUID,
  lease_until TIMESTAMPTZ,
  PRIMARY KEY (account_handle, storage_key)
);

CREATE INDEX IF NOT EXISTS orphan_gc_marks_lease_idx
  ON orphan_gc_marks(lease_until, marked_at);
