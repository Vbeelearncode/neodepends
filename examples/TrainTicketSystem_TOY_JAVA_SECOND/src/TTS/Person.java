package TTS;

/**
 * Base abstract class for all people in the system
 * Unchanged from FIRST version - good base abstraction
 */
public abstract class Person {
    protected String name;
    protected String id;
    protected String email;
    protected String phone;

    public Person(String name, String id, String email, String phone) {
        this.name = name;
        this.id = id;
        this.email = email;
        this.phone = phone;
    }

    public String getName() { return name; }
    public String getId() { return id; }
    public String getEmail() { return email; }
    public String getPhone() { return phone; }

    public abstract void displayInfo();
}
