// Behaviour htmx doesn't cover: phone tabs, the service sheet, a styled confirm dialog and
// toasts. Data still flows through htmx (polling, HX-Trigger events).
(() => {
  "use strict";

  const app = document.querySelector(".app");
  const sheet = document.getElementById("sheet");
  const confirmEl = document.getElementById("confirm");
  const confirmText = document.getElementById("confirm-text");
  const toastEl = document.getElementById("toast");

  // ── Tabs (phone) ────────────────────────────────────────────────
  const tabs = document.querySelectorAll("[data-tab]");
  function show(view) {
    app.dataset.view = view;
    tabs.forEach((t) => t.setAttribute("aria-selected", String(t.dataset.tab === view)));
    try { localStorage.setItem("view", view); } catch {}
    if (view === "processes" || view === "storage" || view === "agents") showWork(view);
  }

  // ── Processes / Storage tabs (laptop) ───────────────────────────
  // On wide screens processes, storage and agents share one area; on phones the bottom tab bar
  // already separates them.
  const workspace = document.querySelector(".workspace");
  const workTabs = document.querySelectorAll("[data-work-tab]");
  function showWork(name) {
    workspace.dataset.work = name;
    workTabs.forEach((t) => t.setAttribute("aria-selected", String(t.dataset.workTab === name)));
    try { localStorage.setItem("work", name); } catch {}
  }
  workTabs.forEach((t) => t.addEventListener("click", () => showWork(t.dataset.workTab)));
  try {
    const saved = localStorage.getItem("work");
    if (saved === "processes" || saved === "storage" || saved === "agents") showWork(saved);
  } catch {}
  tabs.forEach((t) => t.addEventListener("click", () => {
    show(t.dataset.tab);
    window.scrollTo({ top: 0 });
  }));
  try {
    const saved = localStorage.getItem("view");
    if (saved && document.querySelector(`[data-tab="${saved}"]`)) show(saved);
  } catch {}

  // Deep links: #processes or #services opens that tab; #services/<scope>/<unit> also opens
  // that service's sheet, so a failing unit can be bookmarked or shared.
  const hash = decodeURIComponent(location.hash.slice(1));
  const tab = hash.split("/")[0];
  if (document.querySelector(`[data-tab="${tab}"]`)) {
    show(tab);
    // "#processes" also names an element; undo the browser's jump to it, which can land
    // before or after this script runs.
    requestAnimationFrame(() => window.scrollTo(0, 0));
    window.addEventListener("load", () => window.scrollTo(0, 0), { once: true });
  }
  if (/^services\/(system|user)\/[\w:.@\\-]+\.service$/.test(hash)) {
    htmx.ajax("GET", hash, { target: "#sheet-body" });
  }

  // ── Storage ─────────────────────────────────────────────────────
  // A Disk tile opens the storage browser at its mount point.
  function openStorage(path) {
    show("storage");
    htmx.ajax("GET", `storage?path=${encodeURIComponent(path)}`, { target: "#storage", swap: "morph:outerHTML" })
      .then(() => workspace.scrollIntoView({ block: "start", behavior: "smooth" }));
  }
  document.addEventListener("click", (e) => {
    const tile = e.target.closest("[data-storage-path]");
    if (tile) openStorage(tile.dataset.storagePath);
  });
  document.addEventListener("keydown", (e) => {
    const tile = e.target.closest?.("[data-storage-path]");
    if (tile && (e.key === "Enter" || e.key === " ")) {
      e.preventDefault();
      openStorage(tile.dataset.storagePath);
    }
  });

  // ── Service sheet ───────────────────────────────────────────────
  // Rows load a service into #sheet-body; the first load opens the dialog.
  document.body.addEventListener("htmx:afterSwap", (e) => {
    if (e.detail.target.id !== "sheet-body") return;
    const logs = sheet.querySelector("[data-scroll-end]");
    if (logs) logs.scrollTop = logs.scrollHeight;
    if (!sheet.open) sheet.showModal();
  });
  sheet.addEventListener("click", (e) => {
    // A click on the backdrop lands on the dialog element itself.
    if (e.target === sheet || e.target.closest("[data-close]")) sheet.close();
  });
  sheet.addEventListener("close", () => { document.getElementById("sheet-body").replaceChildren(); });
  // After a delete the unit is gone; close its sheet instead of refreshing it.
  document.body.addEventListener("closeSheet", () => { if (sheet.open) sheet.close(); });

  // ── Copy logs ───────────────────────────────────────────────────
  // The clipboard API only exists on HTTPS or localhost, and the dashboard is plain HTTP over
  // WireGuard, so fall back to selecting a hidden textarea and execCommand("copy"). The
  // textarea goes inside the open dialog: a modal makes everything outside it inert.
  async function copyText(text, host) {
    if (navigator.clipboard && window.isSecureContext) {
      await navigator.clipboard.writeText(text);
      return;
    }
    const area = document.createElement("textarea");
    area.value = text;
    area.setAttribute("readonly", "");
    area.style.cssText = "position:fixed;top:0;left:0;width:1px;height:1px;opacity:0;";
    host.append(area);
    area.focus();
    area.setSelectionRange(0, text.length);
    const copied = document.execCommand("copy");
    area.remove();
    if (!copied) throw new Error("copy refused");
  }
  document.addEventListener("click", async (e) => {
    const button = e.target.closest("[data-copy-logs]");
    if (!button) return;
    const logs = button.closest(".logs-block")?.querySelector(".logs");
    if (!logs) return;
    const text = logs.textContent;
    try {
      await copyText(text, button.closest("dialog") ?? document.body);
      const lines = text.trimEnd().split("\n").length;
      toast(`Copied ${lines} log ${lines === 1 ? "line" : "lines"}`);
    } catch {
      toast("Couldn't copy here; press and hold the logs to select them instead", true);
    }
  });

  // ── Confirm ─────────────────────────────────────────────────────
  // Replaces the browser's confirm() for hx-confirm with a native-looking alert.
  document.body.addEventListener("htmx:confirm", (e) => {
    if (!e.detail.question) return;
    e.preventDefault();
    const destructive = /^(Stop|Restart|Kill|Delete)\b/.test(e.detail.question);
    confirmEl.classList.toggle("destructive", destructive);
    confirmEl.querySelector(".confirm-ok").textContent = e.detail.question.split(" ")[0].replace("?", "") || "Continue";
    confirmText.textContent = e.detail.question;
    confirmEl.returnValue = "";
    confirmEl.showModal();
    confirmEl.addEventListener("close", () => {
      if (confirmEl.returnValue === "ok") e.detail.issueRequest(true);
    }, { once: true });
  });

  // ── Toasts ──────────────────────────────────────────────────────
  let toastTimer;
  function toast(message, error = false) {
    toastEl.textContent = message;
    toastEl.classList.toggle("error", error);
    toastEl.classList.add("show");
    clearTimeout(toastTimer);
    // Long messages (a command to copy, say) stay up long enough to read.
    const readable = Math.max(error ? 5000 : 2600, message.length * 60);
    toastTimer = setTimeout(() => toastEl.classList.remove("show"), readable);
  }
  document.body.addEventListener("toast", (e) => toast(e.detail.value ?? e.detail));
  document.body.addEventListener("htmx:responseError", (e) => {
    // Polls failing (say, during a restart of this server) shouldn't nag; actions should.
    if (e.detail.requestConfig.verb === "get" && !e.detail.target.closest("#sheet-body, #sheet")) return;
    toast(e.detail.xhr.responseText || "Something went wrong", true);
  });
  document.body.addEventListener("htmx:sendError", (e) => {
    if (e.detail.requestConfig.verb !== "get") toast("Can't reach the dashboard", true);
  });
})();
