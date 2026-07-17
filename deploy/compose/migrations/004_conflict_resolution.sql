ALTER TABLE snapshots
  ADD COLUMN IF NOT EXISTS resolved_at TIMESTAMPTZ,
  ADD COLUMN IF NOT EXISTS resolved_by_device_cert_id BYTEA,
  ADD COLUMN IF NOT EXISTS resolution_kind TEXT,
  ADD COLUMN IF NOT EXISTS chosen_snapshot_id TEXT;

ALTER TABLE snapshots
  DROP CONSTRAINT IF EXISTS snapshots_resolution_kind_check;
ALTER TABLE snapshots
  ADD CONSTRAINT snapshots_resolution_kind_check
  CHECK (resolution_kind IS NULL OR resolution_kind IN ('keep-cloud-head', 'replace-with-local'));

CREATE INDEX IF NOT EXISTS snapshots_unresolved_conflicts_idx
  ON snapshots(account_handle, logical_save_id, created_at DESC)
  WHERE conflict = TRUE AND resolved_at IS NULL;
