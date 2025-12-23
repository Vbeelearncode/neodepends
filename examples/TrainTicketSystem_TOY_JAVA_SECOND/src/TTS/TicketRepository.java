package TTS;

import java.util.ArrayList;
import java.util.HashMap;

/**
 * NEW CLASS: Repository pattern
 * Manages Ticket entities separately
 */
public class TicketRepository {
    private HashMap<String, Ticket> tickets;

    public TicketRepository() {
        this.tickets = new HashMap<>();
    }

    public void addTicket(Ticket ticket) {
        tickets.put(ticket.getTicketId(), ticket);
    }

    public Ticket getTicket(String ticketId) {
        return tickets.get(ticketId);
    }

    public ArrayList<Ticket> getAllTickets() {
        return new ArrayList<>(tickets.values());
    }

    public ArrayList<Ticket> getTicketsByPassenger(String passengerId) {
        ArrayList<Ticket> result = new ArrayList<>();
        for (Ticket ticket : tickets.values()) {
            if (ticket.getPassengerId().equals(passengerId)) {
                result.add(ticket);
            }
        }
        return result;
    }
}
