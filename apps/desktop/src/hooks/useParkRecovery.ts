import { useEffect } from "react";
import type { Toast } from "@skill-studio/lib";
import { acknowledgeParkRecovery, parkRecoveryNotices, invokeErrorMessage } from "../lib/skill-api";

export function useParkRecovery(addToast: (notice: Omit<Toast, "id">) => string) {
  useEffect(() => {
    let active = true;
    const refresh = () =>
      parkRecoveryNotices()
        .then((report) => {
          if (active && report.interrupted.length > 0) {
            addToast({
              type: "warning",
              title: "Interrupted skill moves",
              message: `Check Activity before retrying. Retained copies: ${report.retained_quarantines.join(", ") || "none recorded"}`,
              duration: Infinity,
              action: {
                label: "Acknowledge",
                onClick: () => {
                  acknowledgeParkRecovery(report.interrupted).catch((error) => {
                    addToast({
                      type: "error",
                      title: "Could not acknowledge recovery",
                      message: invokeErrorMessage(error),
                    });
                  });
                },
              },
            });
          }
        })
        .catch((error) => {
          if (active)
            addToast({
              type: "error",
              title: "Could not read move recovery",
              message: invokeErrorMessage(error),
            });
        });
    refresh();
    window.addEventListener("skill-studio:move-finished", refresh);
    return () => {
      window.removeEventListener("skill-studio:move-finished", refresh);
      active = false;
    };
  }, [addToast]);
}
