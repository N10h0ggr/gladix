
import { DataTable } from "@/components/data-table"

import data from "../app/dashboard/data.json"

export default function PoliciesPage() {
  return (
    <div className="p-6">
      <h1 className="text-2xl font-bold mb-4">Policies</h1>
        <DataTable data={data} />
    </div>
  )
}
