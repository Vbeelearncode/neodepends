package TTS;

/**
 * Base class for staff members
 * Unchanged from FIRST version
 */
public abstract class Staff extends Person {
    protected String employeeId;
    protected double salary;

    public Staff(String name, String id, String email, String phone, String employeeId, double salary) {
        super(name, id, email, phone);
        this.employeeId = employeeId;
        this.salary = salary;
    }

    public String getEmployeeId() { return employeeId; }
    public double getSalary() { return salary; }
}
