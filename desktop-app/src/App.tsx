
import { HashRouter, Routes, Route } from "react-router-dom"
import { SidebarProvider, SidebarInset } from "@/components/ui/sidebar"
import { AppSidebar }                from "@/components/app-sidebar"

import DashboardPage from "./pages/Dashboard"
import TelemetryPage from "./pages/Telemetry" 

export default function Page() {
  return (
    <HashRouter>
      <SidebarProvider>
          <AppSidebar variant="inset" />
          <SidebarInset className="overflow-auto">
            <Routes>
              <Route path="/" element={<DashboardPage />} />
              <Route path="/dashboard" element={<DashboardPage />} />
              <Route path="/telemetry" element={<TelemetryPage />} />
            </Routes>
          </SidebarInset>
      </SidebarProvider>
    </HashRouter>
  )
}
