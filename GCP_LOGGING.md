# GCP Logging Integration

This application supports sending logs to Google Cloud Platform (GCP) Logging. Logs of level `INFO` and above are automatically sent to GCP if credentials are provided.

## How to get GCP Service Account Keys

Since `gcloud` CLI is not installed, follow these steps in the Google Cloud Console:

1.  **Open the GCP Console**: Go to [https://console.cloud.google.com/](https://console.cloud.google.com/).
2.  **Select/Create a Project**: Ensure you have a project selected.
3.  **Go to Service Accounts**: Navigate to **IAM & Admin** > **Service Accounts**.
4.  **Create Service Account**:
    - Click **+ CREATE SERVICE ACCOUNT**.
    - Give it a name (e.g., `Lumen-logger`).
    - Click **CREATE AND CONTINUE**.
5.  **Grant Permissions**:
    - In the **Select a role** dropdown, search for and select **Logging** > **Logs Writer**.
    - Click **CONTINUE**, then click **DONE**.
6.  **Create and Download JSON Key**:
    - Find your new service account in the list and click on its name.
    - Go to the **KEYS** tab.
    - Click **ADD KEY** > **Create new key**.
    - Select **JSON** and click **CREATE**.
    - A JSON file will be downloaded to your computer.

## Installing the Keys

The application looks for the credentials in two places:

1.  **Environment Variable**: Set `GOOGLE_APPLICATION_CREDENTIALS` to the absolute path of the downloaded JSON file.
    ```bash
    export GOOGLE_APPLICATION_CREDENTIALS="/path/to/your/key.json"
    ```
2.  **Default Path**: Place the JSON file at `~/.secrets/service-account.json`.
    ```bash
    mkdir -p ~/.secrets
    cp /path/to/downloaded/key.json ~/.secrets/service-account.json
    ```

## Lua Logging API

The application exposes a global `logging` table to Lua scripts:

- `logging.debug(message)`: Logs a debug message (Console only).
- `logging.info(message)`: Logs an info message (Console + GCP).
- `logging.warn(message)`: Logs a warning message (Console + GCP).
- `logging.error(message)`: Logs an error message (Console + GCP).
- `logging.fatal(message)`: Logs a fatal message (Console + GCP, as CRITICAL severity).

Example:
```lua
logging.info("Starting application...")
if not config then
    logging.error("Configuration missing!")
end
```
