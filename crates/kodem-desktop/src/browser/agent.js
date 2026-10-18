// The page script of Kodem's browser. It runs before the scripts of each page and serves the
// browser tools of agents: an outline of the page with refs to its elements, actions on those
// elements, and the console messages of the page. Replies go back over Kodem's `kodem` message
// handler of WebKit, or WebView2's channel on Windows, with the nonce of the command, so a page
// cannot answer a command that it did not receive.
(() => {
  // WebView2 runs the script in frames too, which serve no commands.
  if (window.__kodem || window !== window.top) {
    return;
  }
  // Taken before the page's scripts run, so that they cannot replace it.
  const channel = window.webkit?.messageHandlers?.kodem ?? window.chrome.webview;
  const LOG_LIMIT = 300;
  const LINE_LIMIT = 1200;
  const TEXT_LIMIT = 60000;
  const logs = [];
  let unread = 0;

  const format = (value) => {
    try {
      if (typeof value === "string") {
        return value;
      }
      if (value instanceof Error) {
        return value.stack || String(value);
      }
      const json = JSON.stringify(value);
      return json === undefined ? String(value) : json;
    } catch {
      return String(value);
    }
  };
  const record = (level, values) => {
    logs.push({ level, text: values.map(format).join(" ").slice(0, 4000) });
    if (logs.length > LOG_LIMIT) {
      logs.shift();
    }
    unread = Math.min(unread + 1, logs.length);
  };
  for (const level of ["log", "info", "warn", "error", "debug"]) {
    const original = console[level];
    console[level] = function (...values) {
      record(level, values);
      return original.apply(this, values);
    };
  }
  addEventListener(
    "error",
    (event) => {
      const target = event.target;
      if (target && target !== window && (target.src || target.href)) {
        record("error", [`Failed to load ${target.tagName.toLowerCase()} ${target.src || target.href}`]);
      } else if (event.message) {
        const place = event.filename ? ` (${event.filename}:${event.lineno})` : "";
        record("error", [event.message + place]);
      }
    },
    true,
  );
  addEventListener("unhandledrejection", (event) => {
    record("error", ["Unhandled promise rejection: " + format(event.reason)]);
  });
  // The embedded browser shows no dialogs, so they are accepted and reported to the agent.
  window.alert = (message) => {
    record("dialog", ["alert: " + format(message)]);
  };
  window.confirm = (message) => {
    record("dialog", ["confirm, accepted: " + format(message)]);
    return true;
  };
  window.prompt = (message, value) => {
    record("dialog", ["prompt, answered with its default: " + format(message)]);
    return value ?? "";
  };

  // The window zooms out a page whose content is wider than the pane, so that it fits from
  // left to right. The page reports the width that its content needs: the right edge of the
  // boxes in the flow, without fixed boxes, the content that a box clips or scrolls, such as a
  // news ticker, and boxes that start right of the view, such as a closed side menu.
  const contentWidth = () => {
    // The document's scroll width is not used: it counts boxes that the page hides off the
    // view and clips.
    let right = 0;
    if (!document.body) {
      return right;
    }
    const walker = document.createTreeWalker(document.body, NodeFilter.SHOW_ELEMENT, {
      acceptNode(element) {
        const style = getComputedStyle(element);
        if (style.display === "none" || style.position === "fixed") {
          return NodeFilter.FILTER_REJECT;
        }
        const rect = element.getBoundingClientRect();
        if (style.visibility === "visible" && rect.width > 0 && rect.height > 0 && rect.left < innerWidth) {
          right = Math.max(right, rect.right + scrollX);
        }
        return style.overflowX === "visible" ? NodeFilter.FILTER_ACCEPT : NodeFilter.FILTER_REJECT;
      },
    });
    while (walker.nextNode()) {
      // The filter measures each box.
    }
    return Math.ceil(right);
  };
  let measured = false;
  const measure = () => {
    channel.postMessage(
      JSON.stringify({ fit: contentWidth(), viewport: innerWidth, fresh: !measured }),
    );
    measured = true;
  };
  let pendingMeasure = 0;
  const scheduleMeasure = () => {
    clearTimeout(pendingMeasure);
    pendingMeasure = setTimeout(measure, 150);
  };
  addEventListener("DOMContentLoaded", scheduleMeasure);
  addEventListener("resize", scheduleMeasure);
  // Late styles, fonts and images change the width after the page loads.
  addEventListener("load", () => {
    scheduleMeasure();
    setTimeout(measure, 1500);
  });

  const clean = (text, limit = 120) => {
    const value = (text || "").replace(/\s+/g, " ").trim();
    return value.length > limit ? value.slice(0, limit - 1) + "…" : value;
  };
  const quote = (text) => JSON.stringify(text);

  // Refs stay with their element across snapshots; a snapshot keeps only the refs it shows.
  const ids = new WeakMap();
  let refs = new Map();
  let counter = 0;
  const refOf = (element) => {
    let id = ids.get(element);
    if (!id) {
      counter += 1;
      id = "e" + counter;
      ids.set(element, id);
    }
    refs.set(id, new WeakRef(element));
    return id;
  };

  const SKIP = new Set(["SCRIPT", "STYLE", "NOSCRIPT", "TEMPLATE", "HEAD", "META", "LINK", "BR"]);
  const INTERACTIVE = new Set([
    "link", "button", "checkbox", "radio", "textbox", "searchbox", "combobox", "listbox",
    "option", "slider", "spinbutton", "switch", "tab", "menuitem", "menuitemcheckbox",
    "menuitemradio", "treeitem", "clickable",
  ]);
  // Roles whose name is their text, so their children are not listed again.
  const NAMED_BY_CONTENT = new Set([
    "link", "button", "heading", "tab", "menuitem", "menuitemcheckbox", "menuitemradio",
    "option", "treeitem", "switch",
  ]);
  const LANDMARKS = {
    NAV: "navigation", MAIN: "main", ASIDE: "complementary", FORM: "form", DIALOG: "dialog",
    UL: "list", OL: "list", LI: "listitem", TABLE: "table", TR: "row", TH: "columnheader",
    TD: "cell", IFRAME: "iframe", SUMMARY: "button", TEXTAREA: "textbox", BUTTON: "button",
    HEADER: "banner", FOOTER: "contentinfo",
  };
  const INPUTS = {
    checkbox: "checkbox", radio: "radio", button: "button", submit: "button", reset: "button",
    image: "button", file: "button", range: "slider", search: "searchbox", number: "spinbutton",
    hidden: null,
  };

  const roleOf = (element, style) => {
    const explicit = element.getAttribute("role");
    if (explicit) {
      return explicit.split(" ")[0];
    }
    const tag = element.tagName;
    if (/^H[1-6]$/.test(tag)) {
      return "heading";
    }
    switch (tag) {
      case "A":
        return element.hasAttribute("href") ? "link" : null;
      case "INPUT": {
        const type = (element.type || "text").toLowerCase();
        return type in INPUTS ? INPUTS[type] : "textbox";
      }
      case "SELECT":
        return element.multiple || element.size > 1 ? "listbox" : "combobox";
      case "IMG":
        return element.getAttribute("alt") === "" ? null : "img";
      default:
        break;
    }
    if (tag in LANDMARKS) {
      return LANDMARKS[tag];
    }
    if (element.isContentEditable && element.hasAttribute("contenteditable")) {
      return "textbox";
    }
    // An element that only script makes clickable shows the pointer cursor, unlike its parent.
    if (
      style.cursor === "pointer" &&
      (element.hasAttribute("onclick") || element.tabIndex >= 0 ||
        (element.parentElement && getComputedStyle(element.parentElement).cursor !== "pointer"))
    ) {
      return "clickable";
    }
    return null;
  };

  const nameOf = (element, role) => {
    const label = element.getAttribute("aria-label");
    if (label && clean(label)) {
      return clean(label);
    }
    const labelledBy = element.getAttribute("aria-labelledby");
    if (labelledBy) {
      const text = labelledBy
        .split(/\s+/)
        .map((id) => document.getElementById(id)?.textContent || "")
        .join(" ");
      if (clean(text)) {
        return clean(text);
      }
    }
    if (element.labels && element.labels.length) {
      const text = [...element.labels].map((label) => label.innerText).join(" ");
      if (clean(text)) {
        return clean(text);
      }
    }
    const alt = element.getAttribute("alt");
    if (alt && clean(alt)) {
      return clean(alt);
    }
    if (NAMED_BY_CONTENT.has(role)) {
      const text = clean(element.innerText ?? element.textContent);
      if (text) {
        return text;
      }
    }
    if (element.tagName === "INPUT" && ["submit", "button", "reset"].includes(element.type)) {
      return clean(element.value);
    }
    return clean(element.getAttribute("title") || element.getAttribute("placeholder") || "");
  };

  // The states and values that a line of the outline shows after the name.
  const detailsOf = (element, role) => {
    const details = [];
    if (role === "heading") {
      const level = element.getAttribute("aria-level") || element.tagName.slice(1);
      details.push(`[level=${level}]`);
    }
    if (role === "checkbox" || role === "radio" || role === "switch") {
      const checked = element.checked ?? element.getAttribute("aria-checked") === "true";
      if (checked) {
        details.push("[checked]");
      }
    }
    const expanded = element.getAttribute("aria-expanded");
    if (expanded) {
      details.push(`[expanded=${expanded}]`);
    }
    if (element.getAttribute("aria-selected") === "true" || element.selected) {
      details.push("[selected]");
    }
    if (element.disabled || element.getAttribute("aria-disabled") === "true") {
      details.push("[disabled]");
    }
    if (role === "link") {
      const href = element.getAttribute("href") || "";
      if (href && !href.startsWith("javascript:")) {
        details.push(`[url=${clean(href, 100)}]`);
      }
    }
    if (INTERACTIVE.has(role)) {
      details.push(`[ref=${refOf(element)}]`);
    }
    if (role === "textbox" || role === "searchbox" || role === "spinbutton" || role === "slider") {
      const value = element.isContentEditable && !("value" in element)
        ? element.innerText
        : element.value;
      if (value) {
        details.push(
          element.type === "password" ? "value=\"••••\"" : `value=${quote(clean(value, 200))}`,
        );
      }
    }
    if (element.tagName === "SELECT") {
      const chosen = [...element.selectedOptions].map((option) => clean(option.label)).join(", ");
      if (chosen) {
        details.push(`value=${quote(chosen)}`);
      }
    }
    return details;
  };

  // The nodes that render inside an element: its shadow tree, with slots replaced by the nodes
  // assigned to them, or its children.
  const childrenOf = (node) => {
    const nodes = node.shadowRoot ? node.shadowRoot.childNodes : node.childNodes;
    const result = [];
    for (const child of nodes) {
      if (child.nodeType === Node.ELEMENT_NODE && child.tagName === "SLOT") {
        const assigned = child.assignedNodes({ flatten: true });
        result.push(...(assigned.length ? assigned : child.childNodes));
      } else {
        result.push(child);
      }
    }
    return result;
  };

  const snapshot = () => {
    refs = new Map();
    const lines = [];
    let size = 0;
    let truncated = false;
    let pending = "";
    let pendingDepth = 0;
    const emit = (depth, line) => {
      if (lines.length >= LINE_LIMIT || size >= TEXT_LIMIT) {
        truncated = true;
        return;
      }
      const text = "  ".repeat(depth) + "- " + line;
      size += text.length + 1;
      lines.push(text);
    };
    const flush = () => {
      const text = clean(pending, 400);
      if (text) {
        emit(pendingDepth, "text: " + quote(text));
      }
      pending = "";
    };
    const walk = (node, depth) => {
      for (const child of childrenOf(node)) {
        if (child.nodeType === Node.TEXT_NODE) {
          if (child.textContent.trim()) {
            if (!pending) {
              pendingDepth = depth;
            }
            pending += child.textContent;
          }
        } else if (child.nodeType === Node.ELEMENT_NODE) {
          visit(child, depth);
        }
      }
    };
    const visit = (element, depth) => {
      if (SKIP.has(element.tagName) || element.getAttribute("aria-hidden") === "true") {
        return;
      }
      const style = getComputedStyle(element);
      if (style.display === "none") {
        return;
      }
      const block = !style.display.startsWith("inline") && style.display !== "contents";
      const shown =
        style.visibility !== "hidden" &&
        style.visibility !== "collapse" &&
        (style.display === "contents" || element.getClientRects().length > 0);
      const role = shown ? roleOf(element, style) : null;
      if (!role) {
        if (block) {
          flush();
        }
        walk(element, depth);
        if (block) {
          flush();
        }
        return;
      }
      flush();
      const name = nameOf(element, role);
      emit(depth, [role, name ? quote(name) : "", ...detailsOf(element, role)].filter(Boolean).join(" "));
      if (element.tagName === "SELECT") {
        for (const option of [...element.options].slice(0, 50)) {
          emit(depth + 1, `option ${quote(clean(option.label))}${option.selected ? " [selected]" : ""}`);
        }
        return;
      }
      if (element.tagName === "IFRAME") {
        let inner = null;
        try {
          inner = element.contentDocument;
        } catch {
          inner = null;
        }
        if (inner && inner.body) {
          walk(inner.body, depth + 1);
          flush();
        } else {
          emit(depth + 1, `text: "Another site's frame: ${clean(element.src, 100)}"`);
        }
        return;
      }
      if (!NAMED_BY_CONTENT.has(role) && role !== "textbox" && role !== "img") {
        walk(element, depth + 1);
        flush();
      }
    };
    if (document.body) {
      walk(document.body, 0);
      flush();
    }
    const scroll = document.scrollingElement || document.documentElement;
    const header = [
      `Page: ${document.title}`,
      `URL: ${location.href}`,
      `Viewport: ${innerWidth}×${innerHeight}, scrolled to ${Math.round(scrollY)} of ${scroll.scrollHeight}`,
    ];
    if (unread) {
      const count = unread === 1 ? "1 new message" : `${unread} new messages`;
      header.push(`Console: ${count}; read them with browser_console_messages.`);
    }
    if (truncated) {
      lines.push("- text: \"The outline is cut here. Scroll, or use browser_evaluate for the rest.\"");
    }
    return header.join("\n") + "\n\n" + (lines.join("\n") || "- text: \"The page shows no content.\"");
  };

  // The agent's cursor. It glides to each element that an agent acts on, frames it, shows each
  // click, and says what the agent does, also the text as it is typed, so that the user can
  // follow the agent. It lies in a closed shadow tree beside the body and takes no pointer
  // events, so the outline, the width measure and hit tests do not see it.
  const motion = !matchMedia("(prefers-reduced-motion: reduce)").matches;
  const GLIDE = 420;
  const LINGER = 2500;
  const CURSOR_STYLE = `
    .cursor { position: fixed; left: 0; top: 0; opacity: 0; will-change: transform;
      transition: transform ${GLIDE}ms cubic-bezier(.22, .8, .26, 1), opacity 180ms ease; }
    .cursor.shown { opacity: 1; }
    .cursor svg { display: block; width: 22px; height: 22px;
      filter: drop-shadow(0 1px 2px rgba(0, 0, 0, .45)); }
    .label { position: absolute; left: 18px; top: 20px; max-width: 280px; padding: 4px 9px;
      border-radius: 8px; background: #ff9447; color: #1b1208; white-space: nowrap;
      overflow: hidden; text-overflow: ellipsis; box-shadow: 0 2px 10px rgba(0, 0, 0, .28);
      font: 600 12px/1.35 -apple-system, BlinkMacSystemFont, "Helvetica Neue", sans-serif; }
    .label:empty { display: none; }
    .ring { position: fixed; width: 36px; height: 36px; margin: -18px 0 0 -18px;
      border: 2px solid #ff9447; border-radius: 50%; opacity: 0; }
    .ring.click { animation: ring 520ms ease-out; }
    @keyframes ring { from { transform: scale(.25); opacity: .95; }
      to { transform: scale(1.5); opacity: 0; } }
    .frame { position: fixed; border: 2px solid #ff9447; border-radius: 6px; opacity: 0;
      box-shadow: 0 0 0 4px rgba(255, 148, 71, .2); transition: opacity 250ms ease; }
    .frame.shown { opacity: 1; }
    .still, .still * { transition: none !important; animation: none !important; }`;
  const CURSOR_ARROW =
    '<svg viewBox="0 0 22 22"><path d="M3 2.5 L3 18 L7.2 14.1 L10 20.2 L12.9 18.9 L10.2 12.9 L16 12.6 Z" ' +
    'fill="#ff9447" stroke="#fff" stroke-width="1.5" stroke-linejoin="round"/></svg>';
  let overlay = null;
  let cursorAt = null;
  let lingering = 0;
  let framing = 0;
  // Builds the overlay, again when the page removed it.
  const cursorParts = () => {
    if (overlay && overlay.host.isConnected) {
      return overlay;
    }
    const host = document.createElement("kodem-agent-cursor");
    host.style.cssText =
      "all: initial; position: fixed; inset: 0; pointer-events: none; z-index: 2147483647;";
    const root = host.attachShadow({ mode: "closed" });
    root.innerHTML =
      `<style>${CURSOR_STYLE}</style><div class="frame"></div><div class="ring"></div>` +
      `<div class="cursor${motion ? "" : " still"}">${CURSOR_ARROW}<div class="label"></div></div>`;
    document.documentElement.appendChild(host);
    overlay = {
      host,
      cursor: root.querySelector(".cursor"),
      label: root.querySelector(".label"),
      ring: root.querySelector(".ring"),
      frame: root.querySelector(".frame"),
    };
    return overlay;
  };
  // WebKit slows the timers of a page out of view to about one a second, and nobody sees its
  // cursor then, so a hidden page acts at once.
  const pause = (ms) =>
    motion && ms > 0 && document.visibilityState === "visible"
      ? new Promise((done) => setTimeout(done, ms))
      : Promise.resolve();
  const conceal = () => {
    if (overlay) {
      overlay.host.style.display = "none";
      overlay.cursor.classList.remove("shown");
      overlay.frame.classList.remove("shown");
    }
  };
  // Shows the cursor where it is, or where it last was on this site, with a caption.
  const announce = (caption) => {
    const parts = cursorParts();
    parts.host.style.display = "";
    if (!cursorAt) {
      let saved = null;
      try {
        saved = JSON.parse(sessionStorage.getItem("__kodemCursor") || "null");
      } catch {
        saved = null;
      }
      cursorAt = saved && Number.isFinite(saved.x) && Number.isFinite(saved.y)
        ? { x: Math.min(saved.x, innerWidth - 24), y: Math.min(saved.y, innerHeight - 24) }
        : { x: innerWidth / 2, y: innerHeight - 64 };
      // The first place takes no glide.
      parts.cursor.classList.add("still");
      parts.cursor.style.transform = `translate(${cursorAt.x}px, ${cursorAt.y}px)`;
      void parts.cursor.offsetWidth;
      if (motion) {
        parts.cursor.classList.remove("still");
      }
    }
    parts.label.textContent = caption;
    parts.cursor.classList.add("shown");
    clearTimeout(lingering);
    lingering = setTimeout(() => {
      parts.cursor.classList.remove("shown");
      parts.frame.classList.remove("shown");
    }, LINGER);
    return parts;
  };
  // Glides the cursor to a point of the view and frames the element there.
  const approach = async (element, point, caption) => {
    const parts = announce(caption);
    const rect = element.getBoundingClientRect();
    Object.assign(parts.frame.style, {
      left: `${rect.left - 3}px`,
      top: `${rect.top - 3}px`,
      width: `${rect.width + 6}px`,
      height: `${rect.height + 6}px`,
    });
    parts.frame.classList.add("shown");
    clearTimeout(framing);
    framing = setTimeout(() => parts.frame.classList.remove("shown"), 1200);
    const distance = Math.hypot(point.x - cursorAt.x, point.y - cursorAt.y);
    parts.cursor.style.transform = `translate(${point.x}px, ${point.y}px)`;
    cursorAt = point;
    try {
      sessionStorage.setItem("__kodemCursor", JSON.stringify(point));
    } catch {
      // A page without storage starts the cursor in the middle.
    }
    await pause(distance > 2 ? GLIDE : 0);
  };
  // Shows a press at a point.
  const ripple = (point) => {
    const parts = cursorParts();
    Object.assign(parts.ring.style, { left: `${point.x}px`, top: `${point.y}px` });
    parts.ring.classList.remove("click");
    void parts.ring.offsetWidth;
    parts.ring.classList.add("click");
  };
  // The caption's name for an element: the agent's own description, or the outline's.
  const subject = (args, element) => (args.element ? quote(clean(args.element, 60)) : describe(element));
  // The caption of typed text: the end of a long text, and dots for a password.
  const typedCaption = (element, typed) => {
    if (element.type === "password") {
      return `Typing ${"•".repeat(Math.min(typed.length, 12))}`;
    }
    const shown = typed.length > 48 ? "…" + typed.slice(-48) : typed;
    return `Typing ${quote(shown)}`;
  };

  const resolve = (args) => {
    if (args.ref) {
      const element = refs.get(args.ref)?.deref();
      if (!element || !element.isConnected) {
        throw new Error(`The ref ${args.ref} is not on the page. Take a new browser_snapshot.`);
      }
      return element;
    }
    if (args.selector) {
      const element = document.querySelector(args.selector);
      if (!element) {
        throw new Error(`No element matches ${args.selector}.`);
      }
      return element;
    }
    throw new Error("Name the element with a ref from browser_snapshot or with a CSS selector.");
  };
  const describe = (element) => {
    const role = roleOf(element, getComputedStyle(element)) || element.tagName.toLowerCase();
    const name = nameOf(element, role);
    return name ? `${role} ${quote(name)}` : role;
  };
  const center = (element) => {
    element.scrollIntoView({ block: "center", inline: "center", behavior: "instant" });
    const rect = element.getBoundingClientRect();
    return { x: rect.left + rect.width / 2, y: rect.top + rect.height / 2 };
  };
  const mouse = (element, type, point, buttons = 0) => {
    const init = {
      bubbles: !type.endsWith("enter"),
      cancelable: true,
      composed: true,
      clientX: point.x,
      clientY: point.y,
      view: window,
      button: 0,
      buttons,
    };
    const event = type.startsWith("pointer")
      ? new PointerEvent(type, { ...init, pointerId: 1, pointerType: "mouse", isPrimary: true })
      : new MouseEvent(type, init);
    element.dispatchEvent(event);
  };
  const hover = (element, point) => {
    for (const type of ["pointerover", "pointerenter", "mouseover", "mouseenter", "pointermove", "mousemove"]) {
      mouse(element, type, point);
    }
  };
  const editable = (element) =>
    element.isContentEditable ||
    element.tagName === "TEXTAREA" ||
    (element.tagName === "INPUT" && !(element.type in INPUTS && element.type !== "search" && element.type !== "number"));

  const click = async (args) => {
    const element = resolve(args);
    // Described before the click, which may change the element's text.
    const name = describe(element);
    if (element.disabled) {
      throw new Error(`${name} is disabled.`);
    }
    let point = center(element);
    await approach(element, point, `${args.double ? "Double-clicking" : "Clicking"} ${subject(args, element)}`);
    // The page may have moved while the cursor glided.
    if (!element.isConnected) {
      throw new Error(`${name} left the page before the click. Take a new browser_snapshot.`);
    }
    const rect = element.getBoundingClientRect();
    point = { x: rect.left + rect.width / 2, y: rect.top + rect.height / 2 };
    ripple(point);
    const top = document.elementFromPoint(point.x, point.y);
    const covered = top && top !== element && !element.contains(top) && !top.contains(element);
    hover(element, point);
    mouse(element, "pointerdown", point, 1);
    mouse(element, "mousedown", point, 1);
    element.focus?.({ preventScroll: true });
    mouse(element, "pointerup", point);
    mouse(element, "mouseup", point);
    element.click();
    if (args.double) {
      mouse(element, "mousedown", point, 1);
      mouse(element, "mouseup", point);
      element.click();
      mouse(element, "dblclick", point);
    }
    const note = covered ? ` Another element covers it: ${describe(top)}.` : "";
    return `Clicked ${name}.${note}`;
  };

  // Sets the text of a field so that frameworks that track the native value see the change.
  const setValue = (element, text) => {
    const prototype = element.tagName === "TEXTAREA" ? HTMLTextAreaElement.prototype : HTMLInputElement.prototype;
    const setter = Object.getOwnPropertyDescriptor(prototype, "value")?.set;
    if (setter) {
      setter.call(element, text);
    } else {
      element.value = text;
    }
    element.dispatchEvent(new InputEvent("input", { bubbles: true, composed: true, inputType: "insertText", data: text }));
  };

  const keyEvent = (element, type, key, modifiers) => {
    const code = key.length === 1
      ? (/[a-z]/i.test(key) ? "Key" + key.toUpperCase() : /[0-9]/.test(key) ? "Digit" + key : "")
      : key;
    return element.dispatchEvent(
      new KeyboardEvent(type, {
        key,
        code,
        bubbles: true,
        cancelable: true,
        composed: true,
        shiftKey: modifiers.has("shift"),
        ctrlKey: modifiers.has("control") || modifiers.has("ctrl"),
        altKey: modifiers.has("alt") || modifiers.has("option"),
        metaKey: modifiers.has("meta") || modifiers.has("cmd") || modifiers.has("command"),
      }),
    );
  };
  const focusable = () =>
    [...document.querySelectorAll("a[href], button, input, select, textarea, [tabindex], [contenteditable]")]
      .filter((element) => !element.disabled && element.tabIndex >= 0 && element.getClientRects().length > 0);

  // Presses a key with its default action where script can perform it: typing a character,
  // deleting one, moving the focus with Tab, and submitting a form or activating a control
  // with Enter.
  const press = async (args) => {
    const parts = String(args.key || "").split("+");
    const key = parts.pop();
    if (!key) {
      throw new Error("Name the key to press.");
    }
    const modifiers = new Set(parts.map((part) => part.toLowerCase()));
    const named = Boolean(args.ref || args.selector);
    const element = named ? resolve(args) : document.activeElement || document.body;
    if (named && !args.quiet) {
      await approach(element, center(element), `Pressing ${args.key} on ${subject(args, element)}`);
    } else if (!args.quiet) {
      announce(`Pressing ${args.key}`);
    }
    if (element !== document.activeElement) {
      element.focus?.({ preventScroll: true });
    }
    const allowed = keyEvent(element, "keydown", key, modifiers);
    if (allowed && key.length === 1 && editable(element)) {
      keyEvent(element, "keypress", key, modifiers);
      if (!document.execCommand("insertText", false, key) && "value" in element) {
        setValue(element, element.value + key);
      }
    } else if (allowed && key === "Backspace" && editable(element)) {
      document.execCommand("delete");
    } else if (allowed && key === "Tab") {
      const list = focusable();
      const index = list.indexOf(element);
      const next = list[(index + (modifiers.has("shift") ? -1 : 1) + list.length) % list.length];
      next?.focus();
    } else if (allowed && key === "Enter") {
      if (element.tagName === "INPUT" && element.form) {
        element.form.requestSubmit();
      } else if (element.tagName === "A" || element.tagName === "BUTTON" || element.getAttribute("role") === "button") {
        element.click();
      } else if (element.tagName !== "TEXTAREA" && !element.isContentEditable) {
        // Nothing to do.
      } else {
        document.execCommand("insertLineBreak");
      }
    }
    keyEvent(element, "keyup", key, modifiers);
    return `Pressed ${args.key} on ${describe(element)}.`;
  };

  // Types text into the focused field in steps that the user can follow, at most about a
  // second and a half for the whole text; past two seconds the rest goes in at once. Tells
  // whether the editor took every step.
  const typeSteps = async (element, text) => {
    const steps = Math.max(1, Math.min(text.length, 40));
    let size = Math.ceil(text.length / steps);
    const delay = Math.min(45, 1400 / steps);
    const deadline = Date.now() + 2000;
    let typed = "";
    for (let index = 0; index < text.length; index += size) {
      if (Date.now() > deadline) {
        size = text.length;
      }
      // The page may move the focus while the agent types; the text goes on at the end.
      if (document.activeElement !== element && !element.contains(document.activeElement)) {
        element.focus({ preventScroll: true });
        if (element.isContentEditable) {
          getSelection()?.collapse(element, element.childNodes.length);
        } else {
          element.setSelectionRange?.(element.value.length, element.value.length);
        }
      }
      const chunk = text.slice(index, index + size);
      if (!document.execCommand("insertText", false, chunk)) {
        return false;
      }
      typed += chunk;
      announce(typedCaption(element, typed));
      await pause(delay);
    }
    return true;
  };

  const type = async (args) => {
    const element = resolve(args);
    const text = String(args.text ?? "");
    if (!editable(element)) {
      throw new Error(`${describe(element)} does not take text.`);
    }
    if (element.disabled || element.readOnly) {
      throw new Error(`${describe(element)} is read-only.`);
    }
    await approach(element, center(element), `Typing into ${subject(args, element)}`);
    ripple(cursorAt);
    element.focus({ preventScroll: true });
    if (element.isContentEditable) {
      if (!args.append) {
        document.execCommand("selectAll");
      } else {
        getSelection()?.collapse(element, element.childNodes.length);
      }
      await typeSteps(element, text);
    } else {
      const value = args.append ? element.value + text : text;
      if (args.append) {
        element.setSelectionRange?.(element.value.length, element.value.length);
      } else {
        element.select?.();
      }
      // Typing through the editor sends the events of real typing; a field that refuses it
      // takes the value directly.
      if (!(await typeSteps(element, text)) || element.value !== value) {
        setValue(element, value);
      }
      element.dispatchEvent(new Event("change", { bubbles: true }));
    }
    if (args.submit) {
      announce("Pressing Enter");
      await press({ key: "Enter", ref: args.ref, selector: args.selector, quiet: true });
    }
    return `Typed into ${describe(element)}.`;
  };

  const select = async (args) => {
    const element = resolve(args);
    if (element.tagName !== "SELECT") {
      throw new Error(`${describe(element)} is not a select element. Click its options instead.`);
    }
    const wanted = (args.values || []).map(String);
    const chosen = [...element.options].filter(
      (option) => wanted.includes(option.value) || wanted.includes(clean(option.label)),
    );
    if (!chosen.length) {
      throw new Error(`No option matches ${wanted.join(", ")}.`);
    }
    const names = chosen.map((option) => quote(clean(option.label))).join(", ");
    await approach(element, center(element), `Choosing ${names}`);
    ripple(cursorAt);
    for (const option of element.options) {
      option.selected = chosen.includes(option) && (element.multiple || option === chosen[0]);
    }
    element.dispatchEvent(new Event("input", { bubbles: true }));
    element.dispatchEvent(new Event("change", { bubbles: true }));
    return `Selected ${names}.`;
  };

  const scroll = async (args) => {
    const target = args.ref || args.selector ? resolve(args) : null;
    const dx = Number(args.dx) || 0;
    const dy = Number(args.dy) || 0;
    if (target && !dx && !dy) {
      await approach(target, center(target), `Scrolling to ${subject(args, target)}`);
      return `Scrolled ${describe(target)} into view.`;
    }
    announce(dy < 0 || dx < 0 ? "Scrolling back" : "Scrolling");
    (target || window).scrollBy({ left: dx, top: dy, behavior: motion ? "smooth" : "instant" });
    await pause(GLIDE);
    return `Scrolled to ${Math.round(scrollY)} of ${(document.scrollingElement || document.documentElement).scrollHeight}.`;
  };

  const text = () => (document.body ? document.body.innerText : "");
  const waitFor = async (args) => {
    const limit = Math.min(Number(args.timeout_seconds) || 10, 30) * 1000;
    const until = Date.now() + limit;
    const met = () =>
      (!args.text || text().includes(args.text)) &&
      (!args.text_gone || !text().includes(args.text_gone)) &&
      (!args.selector || [...document.querySelectorAll(args.selector)].some((e) => e.getClientRects().length > 0));
    if (!args.text && !args.text_gone && !args.selector) {
      throw new Error("Name a text, a text that goes away, or a selector to wait for.");
    }
    announce(`Waiting for ${quote(clean(args.text || args.selector || args.text_gone, 48))}${args.text_gone ? " to go" : ""}`);
    while (!met()) {
      if (Date.now() > until) {
        throw new Error(`The page did not change as expected in ${limit / 1000} s.`);
      }
      await new Promise((done) => setTimeout(done, 100));
    }
    return "The page shows what you waited for.";
  };

  const readConsole = (args) => {
    const list = args.all ? logs : logs.slice(logs.length - unread);
    unread = 0;
    if (!list.length) {
      return args.all ? "The page logged nothing." : "No new messages.";
    }
    return list.map((entry) => `[${entry.level}] ${entry.text}`).join("\n");
  };

  const hoverOver = async (args) => {
    const element = resolve(args);
    const point = center(element);
    await approach(element, point, `Pointing at ${subject(args, element)}`);
    hover(element, point);
    return `Hovered ${describe(element)}.`;
  };

  const readPage = () => {
    announce("Reading the page");
    return snapshot();
  };

  const readLog = (args) => {
    announce("Reading the console");
    return readConsole(args);
  };

  const ACTIONS = {
    snapshot: readPage,
    click,
    hover: hoverOver,
    type,
    select,
    press,
    scroll,
    wait_for: waitFor,
    console: readLog,
  };

  const reply = (nonce, ok, value) => {
    channel.postMessage(JSON.stringify({ nonce, ok, text: String(value) }));
  };
  const run = async (nonce, action, args) => {
    try {
      const handler = ACTIONS[action];
      if (!handler) {
        throw new Error(`The page has no action ${action}.`);
      }
      reply(nonce, true, await handler(args || {}));
    } catch (error) {
      reply(nonce, false, error && error.message ? error.message : String(error));
    }
  };
  const evaluate = async (nonce, body) => {
    try {
      announce("Running a script");
      const value = await body();
      let result;
      if (value === undefined) {
        result = "undefined";
      } else if (value instanceof Element) {
        result = value.outerHTML;
      } else {
        result = JSON.stringify(value, null, 2) ?? String(value);
      }
      reply(nonce, true, result.length > TEXT_LIMIT ? result.slice(0, TEXT_LIMIT) + "\n… cut" : result);
    } catch (error) {
      reply(nonce, false, error && error.stack ? error.stack : String(error));
    }
  };
  Object.defineProperty(window, "__kodem", {
    value: Object.freeze({ run, evaluate, conceal }),
    configurable: false,
    enumerable: false,
    writable: false,
  });
})();
