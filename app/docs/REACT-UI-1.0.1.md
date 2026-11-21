# Architecture

Rust owns app discovery, exact-path process control, profiles, storage, schedule parsing and native window handling. React and Astryx render the embedded frontend in WebView2. IPC accepts commands only from the embedded app origin. No localhost server or npm runtime is required.

The public distribution starts with empty opt-in app lists and no official schedule. App discovery checks a fixed catalog of common paths and requires the user to add each app. Schedule Refresh validates downloaded data before replacement.

Window hiding is best effort: it is not equivalent to a program's native tray setting. The startup policy is one-shot; per-app rehide_after_game is false by default and observes a bounded 15-second iRacing-exit window. Paddock restores cancel pending hides. Trading Paints has a two-second initialization delay.

Tests use synthetic schedules and controlled process/window fixtures. Browser demos use a mocked IPC snapshot and do not launch real companion programs. CM2 footage uses native renderer previews with simulated telemetry; physical hardware behavior is not claimed as verified.
