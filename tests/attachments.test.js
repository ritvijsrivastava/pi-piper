import assert from "node:assert/strict";
import fs from "node:fs";
import test from "node:test";
import vm from "node:vm";

const source = fs.readFileSync(new URL("../assets/attachments.js", import.meta.url), "utf8");
const context = { window: {} };
vm.runInNewContext(source, context, { filename: "assets/attachments.js" });
const attachments = context.window.piPiperAttachments;

test("classifies supported images and text files", () => {
  assert.equal(attachments.kindForFile({ name: "photo.jpg", type: "image/jpeg" }), "image");
  assert.equal(attachments.kindForFile({ name: "photo.webp", type: "" }), "image");
  assert.equal(attachments.kindForFile({ name: "notes.md", type: "text/markdown" }), "text");
  assert.equal(attachments.kindForFile({ name: "config.json", type: "application/octet-stream" }), "text");
  assert.equal(attachments.kindForFile({ name: "archive.zip", type: "application/zip" }), null);
});

test("enforces the requested attachment limits", () => {
  assert.equal(attachments.MAX_FILE_BYTES, 1024 * 1024);
  assert.equal(attachments.MAX_TOTAL_BYTES, 4 * 1024 * 1024);
});

test("builds a fenced text-file prompt with a useful language", () => {
  const block = attachments.buildTextBlock({
    name: "config.json",
    language: attachments.languageForFilename("config.json"),
    text: '{"enabled":true}',
  });
  assert.equal(block, 'Attached file: config.json\n\n```json\n{"enabled":true}\n```');
});

test("uses a safe alternate fence when file content contains backticks", () => {
  const block = attachments.buildTextBlock({
    name: "notes.md",
    language: "markdown",
    text: "```js\nconsole.log(1)\n```",
  });
  assert.match(block, /^Attached file: notes\.md\n\n~~~~markdown/);
});
