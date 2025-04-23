import { DataTable, telemetryDummyData } from "@/components/telemetry/telemetry-table"

export default function TelemetryPage() {
  return (
    <div className="p-6">
      <h1 className="text-2xl font-bold mb-4">EDR Telemetry</h1>
      <DataTable data={telemetryDummyData} />
    </div>
  )
}
