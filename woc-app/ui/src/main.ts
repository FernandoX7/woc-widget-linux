import { mount } from "svelte";
import App from "./App.svelte";
import "./app.css";
import { createFixtureDashboardSnapshot, createFixtureTransport, isFixtureMode, resolveFixtureSelection } from "./lib/fixtures";
import { setTransport } from "./lib/ipc/transport";

const fixtureMode = isFixtureMode();
const fixtureSelection = resolveFixtureSelection();
if (fixtureMode) {
  document.documentElement.dataset.fixture = "true";
  setTransport(createFixtureTransport(fixtureSelection));
}

const app = mount(App, {
  target: document.getElementById("app")!,
  props: {
    initialPage: fixtureMode ? fixtureSelection.page : "overview",
    initialSnapshot: fixtureMode ? createFixtureDashboardSnapshot(fixtureSelection) : undefined,
  },
});

export default app;
