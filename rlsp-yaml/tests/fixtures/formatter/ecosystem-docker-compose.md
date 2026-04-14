---
test-name: ecosystem-docker-compose
category: ecosystem
idempotent: true
---

# Test: Docker Compose Round-Trip

A Docker Compose file with `services:`, `healthcheck.test` flow array
(`["CMD-SHELL", "..."]`), `environment:`, `volumes:`, and `depends_on:`.
Verifies the formatter is idempotent and preserves flow style on the
healthcheck command.

Ref: Docker Compose file reference v3

## Test-Document

```yaml
services:
  db:
    image: postgres:16
    environment:
      POSTGRES_DB: myapp
      POSTGRES_USER: myapp
      POSTGRES_PASSWORD: secret
    volumes:
      - db_data:/var/lib/postgresql/data
    healthcheck:
      test: ["CMD-SHELL", "pg_isready -U myapp"]
      interval: 10s
      timeout: 5s
      retries: 5

  api:
    image: myapp:latest
    environment:
      DATABASE_URL: postgres://myapp:secret@db:5432/myapp
      PORT: "8080"
    ports:
      - 8080:8080
    volumes:
      - ./config:/app/config:ro
    depends_on:
      db:
        condition: service_healthy

volumes:
  db_data: {}
```
