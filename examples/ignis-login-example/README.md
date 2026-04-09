# ignis-login-example

A minimal `ignis_login` example for Ignis.

This project is now a minimal fullstack `ignis_login` example:

- `web` frontend service mounted at `/`
- `api` Rust `http` service mounted at `/api`
- service-level `[services.ignis_login]` on `api`
- two auto-managed secrets injected by igniscloud during deploy:
  - `IGNIS_LOGIN_CLIENT_ID`
  - `IGNIS_LOGIN_CLIENT_SECRET`
- the backend uses the current igniscloud hosted login base URL directly:
  - `https://id.igniscloud.transairobot.com`

The frontend:

- loads the current session from `GET /api/me`
- sends the user to `GET /api/auth/start`
- shows the current user's nickname after login
- calls `POST /api/logout` to clear the session

The backend:

- generates PKCE + state
- redirects to hosted `IgnisCloud ID /login`
- exchanges the authorization code with `/oauth2/token`
- stores the access token in an HttpOnly cookie
- resolves the nickname with `/oidc/userinfo`

## What deploy does

When this service is published and deployed through igniscloud:

1. control-plane reads `[services.ignis_login]`
2. it creates or reuses an `IgnisCloud ID` confidential client
3. it registers the callback URL for `/api/auth/callback`
4. it enables the `google` provider
5. it writes `IGNIS_LOGIN_CLIENT_ID` and `IGNIS_LOGIN_CLIENT_SECRET` into the service secrets

## Build

```bash
cargo check --manifest-path services/api/Cargo.toml
ignis service check --service api
ignis service build --service web
```
