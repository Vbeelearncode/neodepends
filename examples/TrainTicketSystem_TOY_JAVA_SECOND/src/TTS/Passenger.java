package TTS;

import java.util.ArrayList;

/**
 * Passenger entity
 * IMPROVED: Uses ticket IDs instead of Ticket objects to reduce coupling
 */
public class Passenger extends Person {
    private ArrayList<String> ticketIds;  // CHANGED: Was ArrayList<Ticket>

    public Passenger(String name, String id, String email, String phone) {
        super(name, id, email, phone);
        this.ticketIds = new ArrayList<>();
    }

    public void addTicket(String ticketId) {
        ticketIds.add(ticketId);
    }

    public ArrayList<String> getTicketIds() {
        return ticketIds;
    }

    @Override
    public void displayInfo() {
        System.out.println("Passenger: " + name + " (ID: " + id + ")");
        System.out.println("Email: " + email + ", Phone: " + phone);
        System.out.println("Tickets: " + ticketIds.size());
    }
}
