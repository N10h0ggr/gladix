
import { HashRouter, Routes, Route } from "react-router-dom"
import { SidebarProvider, SidebarInset } from "@/components/ui/sidebar"
import { AppSidebar }                from "@/components/app-sidebar"

import DashboardPage from "./pages/Dashboard"
import PoliciesPage  from "./pages/Policies"

export default function Page() {
  return (
    <HashRouter>
      <SidebarProvider>
          <AppSidebar variant="inset" />
          <SidebarInset className="overflow-auto">
            <Routes>
              <Route path="/dashboard" element={<DashboardPage />} />
              <Route path="/policies" element={<PoliciesPage />} />
            </Routes>
          </SidebarInset>
      </SidebarProvider>
    </HashRouter>
  )
}
