# File dialogs, encryption, dialogs and cloud saves

## File dialogs

Open, Save, and folder prompts use the system file dialog. When the machine cannot show one, a path field appears at the top of the editor instead.

In a graph, **Open File Dialog** and **Save File Dialog** ask the player for a path. `path` is empty when they cancel. `filter` on Open File Dialog is a file extension such as `png`, or empty for any file.

## Encryption

Project > Save Games has **Encrypt saves** and **Project key**. With Encrypt saves on, each save file is sealed with that key. A file that was changed, or opened with a different key, does not load.

The **Encrypt** node seals a text. The **Decrypt** node opens it and fails when the text was changed or the key is wrong. Both use the project key. An empty key is left blank until encryption is turned on.

## Dialogs

**Show Dialog** opens a built-in screen. `kind` is `message` (OK), `confirm` (Yes and No), or `input` (a text field and OK). `title`, `message`, and `placeholder` are the words on it. `theme` is a theme file in the project, or empty for the built-in colors.

When the player answers, the `dialog_answered` event runs. **Dialog Answer** reads `button` (`yes`, `no`, or `ok`) and `text`.

## Cloud saves

Project > Save Games has **Cloud saves**, **Provider**, and **Endpoint**.

- Provider **HTTP endpoint** sends slots to the `https://` (TLS, checked against the standard public root certificates) or `http://` address in Endpoint. Host names are resolved by the system. Upload is `POST /upload`. Download is `GET /download?slot=`.
- Provider **Script hook** fires `cloud_hook`. **Cloud Job** reads `kind`, `slot`, `saved_at`, and `body`. **Cloud Reply** finishes the job: `saved_at` is the remote time, and `body` is the remote save for a download.

**Upload Save** and **Download Save** queue a slot. **Flush Cloud Saves** tries the queue. A job that cannot reach the provider stays in `saved/cloud/queue.seal` and is tried again later. When both sides have a save, the newer `saved_at` wins. The events are `cloud_uploaded`, `cloud_downloaded`, `cloud_conflict_local`, and `cloud_conflict_remote`.

An empty endpoint is not an error. Leave it empty when the provider is a script hook.
