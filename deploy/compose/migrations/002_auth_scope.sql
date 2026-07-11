-- Strengthen tenant ownership without rewriting existing identifiers.
-- Rollback: drop the constraints/indexes and snapshots.account_handle after
-- first dropping the dependent composite foreign keys. No encrypted objects
-- or snapshot rows are rewritten by this migration.

ALTER TABLE logical_saves
  ADD CONSTRAINT logical_saves_account_id_unique UNIQUE (account_handle, id);

ALTER TABLE snapshots ADD COLUMN IF NOT EXISTS account_handle BYTEA;
UPDATE snapshots s
SET account_handle = l.account_handle
FROM logical_saves l
WHERE l.id = s.logical_save_id AND s.account_handle IS NULL;
ALTER TABLE snapshots ALTER COLUMN account_handle SET NOT NULL;
ALTER TABLE snapshots
  ADD CONSTRAINT snapshots_account_fk FOREIGN KEY (account_handle)
  REFERENCES accounts(account_handle);
ALTER TABLE snapshots
  ADD CONSTRAINT snapshots_account_logical_fk FOREIGN KEY (account_handle, logical_save_id)
  REFERENCES logical_saves(account_handle, id);
ALTER TABLE snapshots
  ADD CONSTRAINT snapshots_account_id_unique UNIQUE (account_handle, id);

-- A device referenced by an upload must belong to that upload's account.
ALTER TABLE devices
  ADD CONSTRAINT devices_account_cert_unique UNIQUE (account_handle, cert_id);
ALTER TABLE upload_sessions
  ADD CONSTRAINT upload_sessions_account_device_fk
  FOREIGN KEY (account_handle, device_cert_id)
  REFERENCES devices(account_handle, cert_id);

CREATE INDEX IF NOT EXISTS logical_saves_account_updated_idx
  ON logical_saves(account_handle, updated_at DESC);
CREATE INDEX IF NOT EXISTS snapshots_account_logical_created_idx
  ON snapshots(account_handle, logical_save_id, created_at DESC);
CREATE INDEX IF NOT EXISTS auth_challenges_active_idx
  ON auth_challenges(account_handle, device_cert_id, expires_at)
  WHERE used_at IS NULL;
