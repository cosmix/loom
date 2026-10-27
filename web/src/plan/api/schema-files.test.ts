import { describe, expect, it } from "vitest";
import diff from "@/plan/api/fixtures/files-diff.json";
import docChanged from "@/plan/api/fixtures/files-doc-changed.json";
import doc from "@/plan/api/fixtures/files-doc.json";
import docPlan from "@/plan/api/fixtures/files-doc-plan.json";
import saveConflict from "@/plan/api/fixtures/files-save-conflict.json";
import saveInvalid from "@/plan/api/fixtures/files-save-invalid.json";
import saveSaved from "@/plan/api/fixtures/files-save-saved.json";
import search from "@/plan/api/fixtures/files-search.json";
import treeChanged from "@/plan/api/fixtures/files-tree-changed.json";
import tree from "@/plan/api/fixtures/files-tree.json";
import validation from "@/plan/api/fixtures/files-validation.json";
import {
  SAVE_RESULTS,
  diffTextsSchema,
  docChangedSchema,
  docViewSchema,
  fileTreeSchema,
  filesMessageSchemas,
  saveResultSchema,
  searchResultsSchema,
  validationChangedSchema,
} from "./schema-files";

describe("file wire schemas", () => {
  const fixtures = [
    ["tree", fileTreeSchema, tree],
    ["document", docViewSchema, doc],
    ["plan document", docViewSchema, docPlan],
    ["saved", saveResultSchema, saveSaved],
    ["conflict", saveResultSchema, saveConflict],
    ["invalid", saveResultSchema, saveInvalid],
    ["diff", diffTextsSchema, diff],
    ["search", searchResultsSchema, search],
    ["document changed", docChangedSchema, docChanged],
    ["validation", validationChangedSchema, validation],
    ["tree changed", filesMessageSchemas[0], treeChanged],
  ] as const;

  it.each(fixtures)("parses %s and rejects extra fields", (_name, schema, fixture) => {
    expect(schema.safeParse(fixture).success).toBe(true);
    expect(schema.safeParse({ ...fixture, extra: true }).success).toBe(false);
  });

  it("accepts flattened change messages and rejects extra nested fields", () => {
    expect(filesMessageSchemas[1].safeParse({ type: "doc_changed", ...docChanged }).success).toBe(
      true,
    );
    expect(filesMessageSchemas[2].safeParse({ type: "validation", ...validation }).success).toBe(
      true,
    );
    expect(
      docViewSchema.safeParse({ ...doc, sections: [{ ...doc.sections[0], extra: true }] }).success,
    ).toBe(false);
    expect(
      fileTreeSchema.safeParse({
        ...tree,
        knowledge: [
          { ...tree.knowledge[0], children: [{ ...tree.knowledge[0].children[0], extra: true }] },
        ],
      }).success,
    ).toBe(false);
    expect(
      validationChangedSchema.safeParse({
        ...validation,
        validation: {
          ...validation.validation,
          warnings: { ...validation.validation.warnings, extra: true },
        },
      }).success,
    ).toBe(false);
  });

  it("rejects invalid integer ranges and unknown tags", () => {
    expect(docViewSchema.safeParse({ ...doc, size: -1 }).success).toBe(false);
    expect(
      docViewSchema.safeParse({ ...doc, sections: [{ ...doc.sections[0], level: 256 }] }).success,
    ).toBe(false);
    expect(
      fileTreeSchema.safeParse({ ...tree, product: [{ ...tree.product[0], errors: 4294967296 }] })
        .success,
    ).toBe(false);
    expect(saveResultSchema.safeParse({ ...saveSaved, result: "unknown" }).success).toBe(false);
    expect(filesMessageSchemas[0].safeParse({ ...treeChanged, type: "unknown" }).success).toBe(
      false,
    );
  });

  it("keeps save result tags in sync with the union", () => {
    expect(saveResultSchema.options.map((option) => option.shape.result.value).sort()).toEqual(
      [...SAVE_RESULTS].sort(),
    );
  });
});
