// Install the Wails-compat globals (window.go / window.runtime) BEFORE the app
// renders — components call them during mount.
import "./lib/tauri-bridge";

import { render } from "preact";
import { App } from "./app";
import "./styles/main.css";

render(<App />, document.getElementById("app")!);
