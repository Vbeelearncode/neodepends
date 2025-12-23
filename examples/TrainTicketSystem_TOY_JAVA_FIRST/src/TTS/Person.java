package TTS;

/**
 * Base class for all people in the train ticket system
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

    public void setName(String name) { this.name = name; }
    public void setEmail(String email) { this.email = email; }
    public void setPhone(String phone) { this.phone = phone; }

    public abstract void displayInfo();
}
