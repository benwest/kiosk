const { invoke } = window.__TAURI__.core;
const { getCurrentWindow } = window.__TAURI__.window;

const form = document.querySelector("#config-form");
const contentPathEl = document.querySelector("#content-path");
const portEl = document.querySelector("#port");
const statusEl = document.querySelector("#status");

async function loadConfig() {
  const cfg = await invoke("get_config");
  if (cfg) {
    contentPathEl.value = cfg.content_path;
    portEl.value = cfg.port;
  }
}

form.addEventListener("submit", async (e) => {
  e.preventDefault();
  statusEl.textContent = "";
  try {
    await invoke("save_config", {
      contentPath: contentPathEl.value,
      port: portEl.value ? Number(portEl.value) : null,
    });
  } catch (err) {
    statusEl.textContent = String(err);
  }
});

document.querySelector("#quit-btn").addEventListener("click", () => {
  invoke("quit_app");
});

document.querySelector("#browse-btn").addEventListener("click", async () => {
  const folder = await invoke("pick_folder");
  if (folder) {
    contentPathEl.value = folder;
  }
});

getCurrentWindow().listen("config-shown", loadConfig);
window.addEventListener("DOMContentLoaded", loadConfig);
