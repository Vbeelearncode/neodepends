package TTS;

import java.util.ArrayList;

/**
 * Represents a passenger who can book tickets
 */
public class Passenger extends Person {
    private ArrayList<Ticket> bookedTickets;
    private String passportNumber;
    private int loyaltyPoints;

    public Passenger(String name, String id, String email, String phone, String passportNumber) {
        super(name, id, email, phone);
        this.passportNumber = passportNumber;
        this.bookedTickets = new ArrayList<>();
        this.loyaltyPoints = 0;
    }

    public void bookTicket(Ticket ticket) {
        bookedTickets.add(ticket);
        loyaltyPoints += 10;
        System.out.println("Ticket booked successfully for " + name);
    }

    public void cancelTicket(Ticket ticket) {
        if (bookedTickets.remove(ticket)) {
            loyaltyPoints = Math.max(0, loyaltyPoints - 5);
            System.out.println("Ticket cancelled successfully");
        }
    }

    public ArrayList<Ticket> getBookedTickets() {
        return bookedTickets;
    }

    public int getLoyaltyPoints() {
        return loyaltyPoints;
    }

    @Override
    public void displayInfo() {
        System.out.println("Passenger: " + name);
        System.out.println("ID: " + id);
        System.out.println("Loyalty Points: " + loyaltyPoints);
        System.out.println("Total Tickets: " + bookedTickets.size());
    }
}
