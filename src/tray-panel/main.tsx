import { FrontendErrorBoundary } from "@/components/FrontendErrorBoundary";
import { disableContextMenu } from "@/lib/disableContextMenu";
import { installGlobalErrorHandlers } from "@/lib/frontendLogger";
import { createQueryClient } from "@/lib/query/queryClient";
import { QueryClientProvider } from "@tanstack/react-query";
import React from "react";
import ReactDOM from "react-dom/client";
import "../i18n";
import "../index.css";
import "./tray-panel.css";
import { TrayPanel } from "./TrayPanel";
installGlobalErrorHandlers();
disableContextMenu();
const queryClient = createQueryClient({ refetchOnWindowFocus: false });
ReactDOM.createRoot(document.getElementById("root")!).render(
  <React.StrictMode>
    <FrontendErrorBoundary>
      <QueryClientProvider client={queryClient}>
        <TrayPanel />
      </QueryClientProvider>
    </FrontendErrorBoundary>
  </React.StrictMode>,
);
