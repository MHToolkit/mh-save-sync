-- Make tenant identity part of every save/snapshot relationship. This removes
-- the global identifier namespace and permits identical opaque IDs in two
-- accounts without disclosure, squatting, or cross-tenant foreign keys.

ALTER TABLE snapshots DROP CONSTRAINT IF EXISTS snapshots_logical_save_id_fkey;
ALTER TABLE snapshot_parents DROP CONSTRAINT IF EXISTS snapshot_parents_snapshot_id_fkey;
ALTER TABLE snapshot_objects DROP CONSTRAINT IF EXISTS snapshot_objects_snapshot_id_fkey;
ALTER TABLE upload_sessions DROP CONSTRAINT IF EXISTS upload_sessions_logical_save_id_fkey;

ALTER TABLE logical_saves DROP CONSTRAINT IF EXISTS logical_saves_pkey;
ALTER TABLE logical_saves ADD CONSTRAINT logical_saves_pkey PRIMARY KEY (account_handle, id);

ALTER TABLE snapshots DROP CONSTRAINT IF EXISTS snapshots_pkey;
ALTER TABLE snapshots ADD CONSTRAINT snapshots_pkey PRIMARY KEY (account_handle, id);

ALTER TABLE snapshot_parents ADD COLUMN IF NOT EXISTS account_handle BYTEA;
UPDATE snapshot_parents sp
SET account_handle = s.account_handle
FROM snapshots s
WHERE s.id = sp.snapshot_id AND sp.account_handle IS NULL;
ALTER TABLE snapshot_parents ALTER COLUMN account_handle SET NOT NULL;
ALTER TABLE snapshot_parents DROP CONSTRAINT IF EXISTS snapshot_parents_pkey;
ALTER TABLE snapshot_parents
  ADD CONSTRAINT snapshot_parents_pkey
  PRIMARY KEY (account_handle, snapshot_id, parent_snapshot_id);
ALTER TABLE snapshot_parents
  ADD CONSTRAINT snapshot_parents_snapshot_fk
  FOREIGN KEY (account_handle, snapshot_id)
  REFERENCES snapshots(account_handle, id) ON DELETE CASCADE;
ALTER TABLE snapshot_parents
  ADD CONSTRAINT snapshot_parents_parent_fk
  FOREIGN KEY (account_handle, parent_snapshot_id)
  REFERENCES snapshots(account_handle, id);

ALTER TABLE snapshot_objects DROP CONSTRAINT IF EXISTS snapshot_objects_pkey;
ALTER TABLE snapshot_objects
  ADD CONSTRAINT snapshot_objects_pkey
  PRIMARY KEY (account_handle, snapshot_id, object_id);
ALTER TABLE snapshot_objects
  ADD CONSTRAINT snapshot_objects_snapshot_fk
  FOREIGN KEY (account_handle, snapshot_id)
  REFERENCES snapshots(account_handle, id) ON DELETE CASCADE;

ALTER TABLE upload_sessions
  ADD CONSTRAINT upload_sessions_account_logical_fk
  FOREIGN KEY (account_handle, logical_save_id)
  REFERENCES logical_saves(account_handle, id);

ALTER TABLE logical_saves
  ADD CONSTRAINT logical_saves_head_fk
  FOREIGN KEY (account_handle, head_snapshot_id)
  REFERENCES snapshots(account_handle, id)
  DEFERRABLE INITIALLY DEFERRED;

