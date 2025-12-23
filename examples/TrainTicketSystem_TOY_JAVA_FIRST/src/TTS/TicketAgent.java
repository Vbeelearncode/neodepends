package TTS;

/**
 * Ticket agent who can book/cancel tickets for passengers
 */
public class TicketAgent extends Staff {
    private int ticketsProcessed;
    private TrainStation assignedStation;

    public TicketAgent(String name, String id, String email, String phone,
                       String employeeId, double salary) {
        super(name, id, email, phone, employeeId, salary, "Ticketing");
        this.ticketsProcessed = 0;
    }

    public void setAssignedStation(TrainStation station) {
        this.assignedStation = station;
    }

    public Ticket issueTicket(Passenger passenger, Route route, Train train,
                               String seatNumber, double price) {
        Ticket ticket = new Ticket(passenger, route, train, seatNumber, price);
        passenger.bookTicket(ticket);
        ticketsProcessed++;
        return ticket;
    }

    public void cancelTicket(Passenger passenger, Ticket ticket) {
        passenger.cancelTicket(ticket);
        ticket.cancel();
        ticketsProcessed++;
    }

    public int getTicketsProcessed() {
        return ticketsProcessed;
    }

    @Override
    public void performDuties() {
        System.out.println("Processing ticket bookings and cancellations");
    }

    @Override
    public void displayInfo() {
        System.out.println("Ticket Agent: " + name);
        System.out.println("Employee ID: " + employeeId);
        System.out.println("Tickets Processed: " + ticketsProcessed);
    }
}
