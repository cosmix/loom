const EMBEDDED_CONTENT =
  "img, image, picture, video, audio, source, style, script, iframe, object, embed, link, " +
  "form, input, button, textarea, select";

/**
 * Reduces mermaid's rendered svg. mermaid's own sanitiser allows HTML inside `foreignObject`
 * even at securityLevel "strict", so images, styles, form controls and links there are removed (a link keeps
 * its text). Parsed as HTML: mermaid's output carries `<br>` and `&nbsp;`, which an XML parse
 * rejects. Returns null when the markup holds no svg.
 */
export function sanitizeDiagram(svg: string): string | null {
  const doc = new DOMParser().parseFromString(svg, "text/html");
  const root = doc.body.querySelector("svg");
  if (!root) return null;
  for (const object of root.querySelectorAll("*")) {
    // The HTML parser keeps `foreignObject` camel-cased; mermaid's allow-list is lower-cased.
    if (object.localName.toLowerCase() !== "foreignobject") continue;
    for (const embedded of object.querySelectorAll(EMBEDDED_CONTENT)) embedded.remove();
    for (const link of object.querySelectorAll("a")) {
      link.replaceWith(doc.createTextNode(link.textContent ?? ""));
    }
  }
  return root.outerHTML;
}
