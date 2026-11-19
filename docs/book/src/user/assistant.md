# Working with AI assistants

The **Assistant** panel (Window menu) lets AI tools on this computer read and change the open project. It is off until you switch on **Let AI tools connect**.

- The editor listens only on this computer, on port 47700. Set the `SEAL_AGENT_PORT` environment variable to use another port.
- **Connect a tool** shows the snippet to add to your AI tool's MCP settings. The tool starts `seal-editor mcp`, which passes its requests to the running editor.
- With **Ask before each change** on, every change waits in the panel until you press **Allow** or **Deny**. **Allow all** accepts the waiting changes at once.
- **What assistants did** lists every request. The same list is saved in `.seal-editor/assistant-log.txt` inside the project.
