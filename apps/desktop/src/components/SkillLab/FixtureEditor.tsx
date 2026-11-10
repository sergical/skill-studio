import { useState } from "react";
import { Button, Input, Textarea } from "@skill-studio/ui";
import type { FixtureFile } from "@skill-studio/lib";
import { id } from "./lab-model";

interface FileRow {
  id: string;
  file: FixtureFile;
}

export function FixtureEditor({
  files,
  onChange,
}: {
  files: FixtureFile[];
  onChange: (files: FixtureFile[]) => void;
}) {
  const [rows, setRows] = useState<FileRow[]>(() =>
    files.map((file) => ({ id: id("file"), file })),
  );
  const change = (next: FileRow[]) => {
    setRows(next);
    onChange(next.map((row) => row.file));
  };
  const addFile = () => {
    const paths = new Set(rows.map((row) => row.file.path));
    let path = "input.md";
    for (let number = 2; paths.has(path); number++) path = `input-${number}.txt`;
    change([...rows, { id: id("file"), file: { path, content: "" } }]);
  };
  return (
    <div className="space-y-2">
      <div className="flex items-center justify-between">
        <h4 className="text-body font-medium">
          Input files <span className="font-normal text-text-secondary">(optional)</span>
        </h4>
        <Button type="button" variant="ghost" onClick={addFile}>
          Add file
        </Button>
      </div>
      {rows.map((row, index) => (
        <div key={row.id} className="space-y-2 rounded-md bg-bg-secondary p-3">
          <div className="flex gap-2">
            <Input
              aria-label={`File ${index + 1} path`}
              value={row.file.path}
              required
              onChange={(event) =>
                change(
                  rows.map((item) =>
                    item.id === row.id
                      ? { ...item, file: { ...item.file, path: event.target.value } }
                      : item,
                  ),
                )
              }
            />
            <Button
              type="button"
              variant="ghost"
              aria-label={`Remove file ${index + 1}`}
              onClick={() => change(rows.filter((item) => item.id !== row.id))}
            >
              Remove
            </Button>
          </div>
          <Textarea
            aria-label={`File ${index + 1} contents`}
            rows={3}
            value={row.file.content}
            onChange={(event) =>
              change(
                rows.map((item) =>
                  item.id === row.id
                    ? { ...item, file: { ...item.file, content: event.target.value } }
                    : item,
                ),
              )
            }
          />
        </div>
      ))}
    </div>
  );
}
