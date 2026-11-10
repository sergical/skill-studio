import { useEffect } from "react";
import { toast } from "sonner";
import type { Toast } from "@skill-studio/lib";
import { acknowledgeParkRecovery, parkRecoveryNotices, invokeErrorMessage } from "../lib/skill-api";

export function useParkRecovery(addToast: (notice: Omit<Toast, "id">) => string) {
  useEffect(() => {
    let active = true;
    const notices = new Map<string, string>();
    const refresh = () =>
      parkRecoveryNotices()
        .then((report) => {
          if (active && report.interrupted.length > 0) {
            for (const recordId of report.interrupted) {
              if (notices.has(recordId)) continue;
              const toastId = addToast({
                type: "warning",
                title: "Interrupted skill moves",
                message: `Check Activity before retrying. Retained copies: ${report.retained_quarantines.join(", ") || "none recorded"}`,
                duration: Infinity,
                action: {
                  label: "Acknowledge",
                  onClick: () => {
                    acknowledgeParkRecovery([recordId])
                      .then(() => {
                        const id = notices.get(recordId);
                        if (id) toast.dismiss(id);
                      })
                      .catch((error) => {
                        addToast({
                          type: "error",
                          title: "Could not acknowledge recovery",
                          message: invokeErrorMessage(error),
                        });
                      });
                  },
                },
              });
              notices.set(recordId, toastId);
            }
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
      for (const id of notices.values()) toast.dismiss(id);
    };
  }, [addToast]);
}
