package TTS;

/**
 * StationManager - staff role
 * IMPROVED: Reduced to just employee data
 * Management logic moved to ManagementService
 */
public class StationManager extends Staff {
    private String managedStationId;  // CHANGED: Was TrainStation object

    public StationManager(String name, String id, String email, String phone,
                          String employeeId, double salary, String managedStationId) {
        super(name, id, email, phone, employeeId, salary);
        this.managedStationId = managedStationId;
    }

    public String getManagedStationId() { return managedStationId; }

    @Override
    public void displayInfo() {
        System.out.println("Station Manager: " + name + " (ID: " + employeeId + ")");
        System.out.println("Managed Station: " + managedStationId);
        System.out.println("Salary: $" + salary);
    }
}
