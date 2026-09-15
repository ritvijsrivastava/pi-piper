// Browser-side attachment helpers. Files stay in memory and are sent only
// when the user submits the prompt over the already-authenticated WebSocket.
(() => {
  const MAX_FILE_BYTES = 1 * 1024 * 1024;
  const MAX_TOTAL_BYTES = 4 * 1024 * 1024;
  const IMAGE_TYPES = new Set(["image/jpeg", "image/png", "image/webp", "image/gif"]);
  const IMAGE_MIME_BY_EXTENSION = { gif: "image/gif", jpeg: "image/jpeg", jpg: "image/jpeg", png: "image/png", webp: "image/webp" };
  const TEXT_EXTENSIONS = new Set([
    "c", "cc", "conf", "cpp", "css", "csv", "env", "go", "h", "html", "ini", "java",
    "js", "json", "jsx", "log", "md", "mjs", "py", "rb", "rs", "sh", "sql", "svg",
    "toml", "ts", "tsx", "txt", "xml", "yaml", "yml",
  ]);
  const LANGUAGE_BY_EXTENSION = {
    c: "c", cc: "cpp", conf: "text", cpp: "cpp", css: "css", csv: "csv", env: "text",
    go: "go", h: "c", html: "html", ini: "ini", java: "java", js: "javascript",
    json: "json", jsx: "jsx", log: "text", md: "markdown", mjs: "javascript", py: "python",
    rb: "ruby", rs: "rust", sh: "bash", sql: "sql", svg: "xml", toml: "toml", ts: "typescript",
    tsx: "tsx", txt: "text", xml: "xml", yaml: "yaml", yml: "yaml",
  };

  function extensionForFilename(name) {
    const match = /\.([a-z0-9]+)$/i.exec(name || "");
    return match ? match[1].toLowerCase() : "";
  }

  function kindForFile(file) {
    const mimeType = (file?.type || "").toLowerCase();
    const extension = extensionForFilename(file?.name);
    if (IMAGE_TYPES.has(mimeType) || IMAGE_MIME_BY_EXTENSION[extension]) return "image";
    if (mimeType.startsWith("text/") || mimeType === "application/json" || TEXT_EXTENSIONS.has(extension)) {
      return "text";
    }
    return null;
  }

  function languageForFilename(name) {
    return LANGUAGE_BY_EXTENSION[extensionForFilename(name)] || "text";
  }

  function readAsDataUrl(file) {
    return new Promise((resolve, reject) => {
      const reader = new FileReader();
      reader.addEventListener("load", () => resolve(String(reader.result || "")));
      reader.addEventListener("error", () => reject(new Error(`Could not read ${file.name}`)));
      reader.readAsDataURL(file);
    });
  }

  async function readFile(file) {
    if (!file || typeof file.size !== "number") throw new Error("Invalid attachment");
    if (file.size > MAX_FILE_BYTES) throw new Error(`${file.name} is larger than 1 MB`);

    const kind = kindForFile(file);
    if (!kind) throw new Error(`${file.name} is not a supported image or text file`);

    if (kind === "text") {
      return {
        kind,
        name: file.name,
        size: file.size,
        language: languageForFilename(file.name),
        text: await file.text(),
      };
    }

    const dataUrl = await readAsDataUrl(file);
    const separator = dataUrl.indexOf(",");
    if (separator < 0) throw new Error(`Could not read ${file.name}`);
    return {
      kind,
      name: file.name,
      size: file.size,
      mimeType: file.type || IMAGE_MIME_BY_EXTENSION[extensionForFilename(file.name)],
      data: dataUrl.slice(separator + 1),
      dataUrl,
    };
  }

  function buildTextBlock(attachment) {
    const fence = attachment.text.includes("```") ? "~~~~" : "```";
    return `Attached file: ${attachment.name}\n\n${fence}${attachment.language}\n${attachment.text}\n${fence}`;
  }

  window.piPiperAttachments = Object.freeze({
    MAX_FILE_BYTES,
    MAX_TOTAL_BYTES,
    buildTextBlock,
    kindForFile,
    languageForFilename,
    readFile,
  });
})();
