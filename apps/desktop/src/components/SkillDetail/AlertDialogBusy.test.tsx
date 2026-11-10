// @vitest-environment happy-dom

import { cleanup, render, screen } from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import { afterEach, describe, expect, it, vi } from "vitest";
import { AlertDialog, AlertDialogContent, AlertDialogTitle } from "@skill-studio/ui";

function renderConfirm(busy: boolean) {
  const onOpenChange = vi.fn();
  render(
    <AlertDialog open busy={busy} onOpenChange={onOpenChange}>
      <AlertDialogContent>
        <AlertDialogTitle>Remove skill?</AlertDialogTitle>
      </AlertDialogContent>
    </AlertDialog>,
  );
  return onOpenChange;
}

describe("confirm dialog: Escape closes it only when its action is not running", () => {
  afterEach(cleanup);

  // Flow: confirm Remove, then press Escape while the remove still runs.
  // Expect the dialog to stay open so the user sees the result. Failure: the
  // dialog closes mid-remove and the outcome lands with no dialog on screen.
  it("keeps a busy dialog open on Escape", async () => {
    const onOpenChange = renderConfirm(true);
    await screen.findByText("Remove skill?");
    await userEvent.keyboard("{Escape}");
    expect(onOpenChange).not.toHaveBeenCalled();
  });

  it("closes an idle dialog on Escape", async () => {
    const onOpenChange = renderConfirm(false);
    await screen.findByText("Remove skill?");
    await userEvent.keyboard("{Escape}");
    expect(onOpenChange).toHaveBeenCalledWith(false, expect.anything());
  });
});
