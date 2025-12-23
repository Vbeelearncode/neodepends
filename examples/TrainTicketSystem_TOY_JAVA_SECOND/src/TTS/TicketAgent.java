package TTS;

/**
 * TicketAgent - staff role
 * IMPROVED: Reduced to just employee data
 * Business logic moved to BookingService
 */
public class TicketAgent extends Staff {
    private String assignedStationId;  // CHANGED: Was TrainStation object

    public TicketAgent(String name, String id, String email, String phone,
                       String employeeId, double salary, String assignedStationId) {
        super(name, id, email, phone, employeeId, salary);
        this.assignedStationId = assignedStationId;
    }

    public String getAssignedStationId() { return assignedStationId; }

    @Override
    public void displayInfo() {
        System.out.println("Ticket Agent: " + name + " (ID: " + employeeId + ")");
        System.out.println("Assigned Station: " + assignedStationId);
        System.out.println("Salary: $" + salary);
    }
}
