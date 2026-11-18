import { act, cleanup, fireEvent, render, screen, waitFor, within } from "@testing-library/react";
import { Provider, createStore } from "jotai";
import { afterEach, describe, expect, it, vi } from "vitest";

import docFixture from "@/plan/api/fixtures/files-doc.json";
import treeFixture from "@/plan/api/fixtures/files-tree.json";
import { docViewSchema, fileTreeSchema, type FileTree } from "@/plan/api/schema-files";
import { treeAtom } from "@/plan/files/atoms";
import { DocumentStarter } from "@/plan/new/document-starter";

afterEach(() => {
  cleanup();
  vi.unstubAllGlobals();
});

function mount(tree: FileTree = fileTreeSchema.parse(treeFixture)) {
  const store = createStore();
  store.set(treeAtom, tree);
  const onPick = vi.fn();
  render(
    <Provider store={store}>
      <DocumentStarter onPick={onPick} />
    </Provider>,
  );
  fireEvent.click(screen.getByRole("button", { name: "Start from a document" }));
  return onPick;
}

function stubDocument(text: string, title = "Current vision") {
  const doc = docViewSchema.parse({
    ...docFixture,
    path: "doc/product/demo/index.md",
    title,
    text,
    hash: "sha256:current-vision",
    size: new TextEncoder().encode(text).length,
  });
  const fetchMock = vi.fn<typeof fetch>().mockResolvedValue(new Response(JSON.stringify(doc)));
  vi.stubGlobal("fetch", fetchMock);
  return fetchMock;
}

describe("DocumentStarter", () => {
  it("lists nested documents from every tree group and omits folders and missing entries", () => {
    mount();

    const list = within(screen.getByRole("list", { name: "Documents" }));
    expect(list.getAllByRole("button").map((button) => button.textContent)).toEqual([
      "Visiondoc/product/demo/index.md",
      "File modedoc/plans/PLAN-file-mode.md",
      "Overviewdoc/loom/knowledge/architecture/overview.md",
      "Notesdoc/notes.md",
      "Old briefdoc/plans/archive/briefs/old.md",
    ]);
    expect(screen.queryByRole("button", { name: "Section 2" })).toBeNull();
    expect(screen.queryByRole("button", { name: "Architecture" })).toBeNull();
    expect(screen.queryByRole("button", { name: "Orphan briefs" })).toBeNull();
  });

  it("omits folder and gap entries even when they have paths", () => {
    const tree = fileTreeSchema.parse(treeFixture);
    const entry = tree.other[0];
    if (!entry) throw new Error("The fixture needs a document");
    tree.other.push(
      { ...entry, kind: "brief_folder", title: "Folder", path: "doc/briefs" },
      { ...entry, gap: true, title: "Missing", path: "doc/missing.md" },
    );
    mount(tree);

    expect(screen.queryByRole("button", { name: "Folder" })).toBeNull();
    expect(screen.queryByRole("button", { name: "Missing" })).toBeNull();
  });

  it("fetches a document and picks a whole-document source with a capped quote and current hash", async () => {
    const fetchMock = stubDocument("x".repeat(2_001));
    const onPick = mount();

    fireEvent.click(screen.getByRole("button", { name: "Vision" }));

    await waitFor(() => expect(onPick).toHaveBeenCalledTimes(1));
    expect(fetchMock).toHaveBeenCalledWith(
      "/api/doc?path=product%2Fdemo%2Findex.md",
      expect.objectContaining({ method: "GET", credentials: "same-origin" }),
    );
    expect(onPick).toHaveBeenCalledWith({
      label: "Current vision",
      attachment: {
        kind: "source",
        path: "doc/product/demo/index.md",
        anchor: null,
        lines: null,
        stage: null,
        summary_sentence: null,
        rests_on: [],
        quote: `${"x".repeat(2_000)}\n[…1 more characters; read the source]`,
        selected_hash: "sha256:current-vision",
      },
    });
    expect(screen.queryByRole("dialog", { name: "Choose a starter document" })).toBeNull();
  });

  it("caps Unicode quotes on character boundaries", async () => {
    stubDocument("😀".repeat(2_001));
    const onPick = mount();

    fireEvent.click(screen.getByRole("button", { name: "Vision" }));

    await waitFor(() =>
      expect(onPick).toHaveBeenCalledWith(
        expect.objectContaining({
          attachment: expect.objectContaining({
            quote: `${"😀".repeat(2_000)}\n[…1 more characters; read the source]`,
          }),
        }),
      ),
    );
  });

  it("preserves a short document quote", async () => {
    stubDocument("# Vision\n\nBuild something useful.\n");
    const onPick = mount();

    fireEvent.click(screen.getByRole("button", { name: "Vision" }));

    await waitFor(() =>
      expect(onPick).toHaveBeenCalledWith(
        expect.objectContaining({
          attachment: expect.objectContaining({ quote: "# Vision\n\nBuild something useful.\n" }),
        }),
      ),
    );
  });

  it("shows a safe fetch error and keeps the picker open for another attempt", async () => {
    vi.stubGlobal(
      "fetch",
      vi.fn<typeof fetch>().mockResolvedValue(
        new Response(JSON.stringify({ error: "unavailable", message: "Cannot read \u202Edoc" }), {
          status: 503,
        }),
      ),
    );
    const onPick = mount();

    fireEvent.click(screen.getByRole("button", { name: "Vision" }));

    expect((await screen.findByRole("alert")).textContent).toContain("Cannot read ⟨U+202E⟩doc");
    expect(screen.getByRole("dialog", { name: "Choose a starter document" })).toBeTruthy();
    expect(screen.getByRole("button", { name: "Vision" }).hasAttribute("disabled")).toBe(false);
    expect(onPick).not.toHaveBeenCalled();

    stubDocument("# Recovered vision");
    fireEvent.click(screen.getByRole("button", { name: "Vision" }));
    await waitFor(() => expect(onPick).toHaveBeenCalledTimes(1));
    expect(screen.queryByRole("alert")).toBeNull();
  });

  it("ignores a pending fetch after the picker closes", async () => {
    let respond: (response: Response) => void = () => {};
    vi.stubGlobal(
      "fetch",
      vi.fn<typeof fetch>().mockImplementation(
        () =>
          new Promise<Response>((resolve) => {
            respond = resolve;
          }),
      ),
    );
    const onPick = mount();
    fireEvent.click(screen.getByRole("button", { name: "Vision" }));
    expect(screen.getByRole("status").textContent).toBe("Loading document…");
    expect(screen.getByRole("button", { name: "Vision" }).hasAttribute("disabled")).toBe(true);

    fireEvent.click(screen.getByRole("button", { name: "Start from a document" }));
    await act(async () => {
      respond(new Response(JSON.stringify(docFixture)));
    });

    expect(screen.queryByRole("dialog", { name: "Choose a starter document" })).toBeNull();
    expect(onPick).not.toHaveBeenCalled();
  });

  it("renders document titles and paths through Untrusted", () => {
    const tree = fileTreeSchema.parse(treeFixture);
    const entry = tree.other[0];
    if (!entry) throw new Error("The fixture needs a document");
    tree.other = [{ ...entry, title: "Notes\u202E", path: "doc/notes\u202E.md" }];
    mount(tree);

    expect(screen.getByRole("button", { name: "Notes⟨U+202E⟩" })).toBeTruthy();
    expect(screen.getByText("doc/notes⟨U+202E⟩.md")).toBeTruthy();
  });

  describe("a file from this computer", () => {
    const stored = {
      id: "0123456789abcdef",
      name: "notes.md",
      media_type: "text/markdown",
      bytes: 5,
    };
    const chosen = () => document.querySelector<HTMLInputElement>('input[type="file"]')!;

    it("offers both choices, and the file input accepts the supported types and several files", () => {
      mount();

      expect(screen.getByRole("button", { name: "A file from this computer…" })).toBeTruthy();
      expect(screen.getByText("A document in this project")).toBeTruthy();
      expect(chosen().multiple).toBe(true);
      expect(chosen().accept).toContain("image/png");
      expect(chosen().accept).toContain(".md");
    });

    it("uploads the chosen file and picks it by id, then closes the menu", async () => {
      const fetchMock = vi
        .fn<typeof fetch>()
        .mockResolvedValue(new Response(JSON.stringify(stored)));
      vi.stubGlobal("fetch", fetchMock);
      const onPick = mount();

      fireEvent.change(chosen(), {
        target: { files: [new File(["hello"], "notes.md", { type: "" })] },
      });

      await waitFor(() => expect(onPick).toHaveBeenCalledTimes(1));
      expect(onPick).toHaveBeenCalledWith({
        label: "notes.md",
        attachment: { kind: "file", id: "0123456789abcdef", name: "notes.md" },
        file: stored,
      });
      const [url, init] = fetchMock.mock.calls[0]!;
      expect(url).toBe("/api/attachments?name=notes.md");
      expect(init?.headers).toEqual({ "Content-Type": "text/markdown" });
      await waitFor(() =>
        expect(screen.queryByRole("dialog", { name: "Choose a starter document" })).toBeNull(),
      );
    });

    it("keeps the menu open and shows the server's reason when the upload is refused", async () => {
      vi.stubGlobal(
        "fetch",
        vi.fn<typeof fetch>().mockResolvedValue(
          new Response(JSON.stringify({ error: "too_large", message: "Over 10 MiB <b>" }), {
            status: 413,
          }),
        ),
      );
      const onPick = mount();

      fireEvent.change(chosen(), {
        target: { files: [new File(["x"], "big.png", { type: "image/png" })] },
      });

      const alert = await screen.findByRole("alert");
      expect(alert.textContent).toBe("big.png: Over 10 MiB <b>");
      expect(alert.querySelector("b")).toBeNull();
      expect(onPick).not.toHaveBeenCalled();
      expect(screen.getByRole("dialog", { name: "Choose a starter document" })).toBeTruthy();
    });

    it("refuses a type outside the list without sending it", async () => {
      const fetchMock = vi.fn<typeof fetch>();
      vi.stubGlobal("fetch", fetchMock);
      mount();

      fireEvent.change(chosen(), {
        target: { files: [new File(["x"], "a.zip", { type: "application/zip" })] },
      });

      expect((await screen.findByRole("alert")).textContent).toContain(
        "a.zip is not a supported type",
      );
      expect(fetchMock).not.toHaveBeenCalled();
    });
  });
});
