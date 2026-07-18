CREATE TABLE IF NOT EXISTS orphan_gc_purge_queue (
  account_handle BYTEA NOT NULL,
  storage_key TEXT NOT NULL,
  queued_at TIMESTAMPTZ NOT NULL DEFAULT now(),
  lease_token UUID,
  lease_until TIMESTAMPTZ,
  PRIMARY KEY (account_handle, storage_key)
);

CREATE INDEX IF NOT EXISTS orphan_gc_purge_queue_lease_idx
  ON orphan_gc_purge_queue(lease_until, queued_at);
