# MH Save Sync self-hosted compose

Five-minute local demo:

```bash
printf 'change-me-local-postgres' > secrets/postgres_password.txt
printf 'minioadmin' > secrets/minio_root_user.txt
printf 'change-me-local-minio-password' > secrets/minio_root_password.txt
chmod 600 secrets/*.txt
docker compose up -d --wait
curl -fsS http://127.0.0.1:18080/ready
```

This stack is for isolated development and disaster-recovery testing. It must not be deployed over `nemessix-room`; use a distinct Compose project name and non-conflicting ports.

Backups require both PostgreSQL and MinIO object data. Restoring only one side must be treated as not ready until `scripts/verify-repository.sh` proves every committed snapshot references durable encrypted objects.
