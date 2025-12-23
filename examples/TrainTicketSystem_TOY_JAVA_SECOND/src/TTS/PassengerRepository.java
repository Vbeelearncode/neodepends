package TTS;

import java.util.ArrayList;
import java.util.HashMap;

/**
 * NEW CLASS: Repository pattern
 * Manages Passenger entities separately
 */
public class PassengerRepository {
    private HashMap<String, Passenger> passengers;

    public PassengerRepository() {
        this.passengers = new HashMap<>();
    }

    public void addPassenger(Passenger passenger) {
        passengers.put(passenger.getId(), passenger);
    }

    public Passenger getPassenger(String passengerId) {
        return passengers.get(passengerId);
    }

    public ArrayList<Passenger> getAllPassengers() {
        return new ArrayList<>(passengers.values());
    }
}
