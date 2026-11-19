# Remote control

Open **Edit → Project Settings**. Under **Remote control**, turn on **Allow HTTP remote control**.

Fields:

- **Allow HTTP remote control** — starts a local server on `127.0.0.1` only.
- **Remote HTTP port** — port for HTTP (default `47810`). WebSocket events use the next port (`port + 1`).
- **Remote access token** — every HTTP request and the web panel must send this token.

While the server is on, open `http://127.0.0.1:<port>/panel?token=<token>` for the built-in panel (play, stop, command box, property list). Put extra HTML panels in the project folder `web_panels/`; they are served under `/panels/`.

API (token in `Authorization: Bearer …` or `?token=`):

- `GET /api/selection` — current selection ids.
- `GET /api/properties` — inspector fields for the primary selection.
- `GET /api/screenshot` — the latest window frame as PNG. A request also asks the window to capture a new frame; before the first capture it answers 503, so ask again.
- `POST /api/command` — JSON `{"id":"<command>","args":{}}` runs an editor command.

WebSocket clients on `port + 1` receive a JSON message `{"type":"selection","ids":[...],"primary":...}` every time the selection changes, from the editor UI or from a remote command.
