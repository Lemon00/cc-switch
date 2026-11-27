import { describe, expect, it } from "vitest";
import { parseDiff } from "@/components/sessions/reader/SessionDiff";
describe("session diff file boundaries", () => {
  it("keeps adjacent removed and added lines that resemble file headers", () => {
    expect(
      parseDiff(
        "--- a/demo.txt\n+++ b/demo.txt\n@@ -1 +1 @@\n--- old content\n+++ new content\n",
      ),
    ).toEqual([
      { type: "file", path: "demo.txt" },
      { type: "del", text: "-- old content", oldNo: 1 },
      { type: "add", text: "++ new content", newNo: 1 },
    ]);
  });
  it("keeps the deleted file name before the following file", () => {
    const lines = parseDiff(
      "diff --git a/removed.txt b/removed.txt\n--- a/removed.txt\n+++ /dev/null\n@@ -1 +0,0 @@\n-removed\ndiff --git a/next.txt b/next.txt\n--- a/next.txt\n+++ b/next.txt\n@@ -1 +1 @@\n-before\n+after\n",
    );
    expect(lines.filter((line) => line.type === "file")).toEqual([
      { type: "file", path: "removed.txt" },
      { type: "file", path: "next.txt" },
    ]);
  });
  it("keeps header-like content in an apply_patch hunk", () => {
    expect(
      parseDiff(
        "*** Begin Patch\n*** Update File: demo.txt\n@@\n--- old content\n+++ new content\n*** End Patch\n",
      ),
    ).toEqual([
      { type: "file", path: "demo.txt" },
      { type: "del", text: "-- old content", oldNo: undefined },
      { type: "add", text: "++ new content", newNo: undefined },
    ]);
  });
});
