# `dual-frontend-login-example`

A three-service Ignis example with one backend and two frontends:

- `api`: Rust `http` service mounted at `/api`
- `app`: Vue frontend mounted at `/`
- `admin`: Vue frontend mounted at `/admin`

What it demonstrates:

- hosted Google login via service-level `ignis_login`
- a simple same-origin test page that still calls `GET /api/hello`
- SQLite-backed persistence of everyone who completed login
- a second admin frontend that lists registered users

## Project

This example uses:

```hcl
project = {
  name = "dual_frontend_login_example"
}
```

## Routes

- App: `/`
- Admin: `/admin`
- API: `/api`
- Login start: `/api/auth/start`
- Login callback: `/api/auth/callback`
- Session: `/api/me`
- Registered users: `/api/users`

## Build

```bash
cargo check --manifest-path services/api/Cargo.toml
ignis service build --service api
ignis service build --service app
ignis service build --service admin
```

## Deploy

```bash
ignis project sync --mode apply
ignis service publish --service api
ignis service publish --service app
ignis service publish --service admin
ignis service deploy --service api <version>
ignis service deploy --service app <version>
ignis service deploy --service admin <version>
```

After deploy:

- the user app lives at `https://<project-id>.<base-domain>/`
- the admin page lives at `https://<project-id>.<base-domain>/admin`
- the API lives at `https://<project-id>.<base-domain>/api`

The admin page currently has no separate role model. It only requires a valid
login session, then shows the list of users who have completed the hosted login
flow and were stored into SQLite.
