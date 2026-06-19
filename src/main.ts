import { mount } from "svelte";
import App from "./App.svelte";
import "./app.css";

// Svelte 5 mount API.
const app = mount(App, {
  target: document.getElementById("app")!,
});

export default app;
