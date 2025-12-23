package TTS;

/**
 * Base class for all staff members
 */
public abstract class Staff extends Person {
    protected String employeeId;
    protected double salary;
    protected String department;

    public Staff(String name, String id, String email, String phone,
                 String employeeId, double salary, String department) {
        super(name, id, email, phone);
        this.employeeId = employeeId;
        this.salary = salary;
        this.department = department;
    }

    public String getEmployeeId() { return employeeId; }
    public double getSalary() { return salary; }
    public String getDepartment() { return department; }

    public abstract void performDuties();
}
