const assert = require("node:assert/strict");
const fs = require("node:fs");
const test = require("node:test");
const vm = require("node:vm");

const context = { window: {} };
vm.runInNewContext(fs.readFileSync("assets/markdown.js", "utf8"), context, {
  filename: "assets/markdown.js",
});
const renderMarkdown = context.window.piPiperMarkdown.renderMarkdown;

test("renders common Markdown blocks and inline formatting", () => {
  const output = renderMarkdown(
    "# Title\n\nA **bold** word with *emphasis* and `inline code`.\n\n- first\n- second\n\n> quoted",
  );

  assert.match(output, /<h1>Title<\/h1>/);
  assert.match(output, /<strong>bold<\/strong>/);
  assert.match(output, /<em>emphasis<\/em>/);
  assert.match(output, /<code>inline code<\/code>/);
  assert.match(output, /<ul><li>first<\/li><li>second<\/li><\/ul>/);
  assert.match(output, /<blockquote>quoted<\/blockquote>/);
});

test("renders fenced code with a language and neutral syntax emphasis", () => {
  const output = renderMarkdown("   ```javascript\n   const count = 1;\n   // comment\n   <script>\n   ```");

  assert.match(output, /<pre class="markdown-code" data-language="javascript">/);
  assert.match(output, /code-token-keyword/);
  assert.match(output, /code-token-number/);
  assert.match(output, /code-token-comment/);
  assert.match(output, /&lt;script/);
});

test("escapes HTML and rejects unsafe links", () => {
  const output = renderMarkdown(
    '[safe](https://example.com) [unsafe](javascript:alert(1))\n\n<script>alert(1)</script>',
  );

  assert.match(output, /href="https:\/\/example\.com"/);
  assert.doesNotMatch(output, /href="javascript:/i);
  assert.doesNotMatch(output, /<script>/i);
  assert.match(output, /&lt;script&gt;/);
});
