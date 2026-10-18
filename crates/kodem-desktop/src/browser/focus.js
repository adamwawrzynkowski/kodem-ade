// Tells Kodem's window when the user presses a page, in any of its frames, and when the page
// takes the keyboard in another way, so that the window's text fields give the keyboard up.
(() => {
  const channel = window.webkit?.messageHandlers?.kodem ?? window.chrome?.webview;
  if (!channel) {
    return;
  }
  const report = (focus) => (event) => {
    if (event.isTrusted) {
      channel.postMessage(JSON.stringify({ focus }));
    }
  };
  addEventListener("pointerdown", report("click"), true);
  // The window of a frame receives focus when the page takes the keyboard.
  addEventListener("focus", report("focus"));
})();
