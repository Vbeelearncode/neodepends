package TTS;

import java.util.ArrayList;

/**
 * Station manager who manages train schedules and operations
 */
public class StationManager extends Staff {
    private TrainStation managedStation;
    private ArrayList<Train> scheduledTrains;

    public StationManager(String name, String id, String email, String phone,
                          String employeeId, double salary) {
        super(name, id, email, phone, employeeId, salary, "Station Management");
        this.scheduledTrains = new ArrayList<>();
    }

    public void setManagedStation(TrainStation station) {
        this.managedStation = station;
    }

    public void addTrainSchedule(Train train) {
        scheduledTrains.add(train);
        System.out.println("Train " + train.getTrainNumber() + " added to schedule");
    }

    public void removeTrainSchedule(Train train) {
        if (scheduledTrains.remove(train)) {
            System.out.println("Train removed from schedule");
        }
    }

    public ArrayList<Train> getScheduledTrains() {
        return scheduledTrains;
    }

    @Override
    public void performDuties() {
        System.out.println("Managing train schedules and station operations");
    }

    @Override
    public void displayInfo() {
        System.out.println("Station Manager: " + name);
        System.out.println("Employee ID: " + employeeId);
        System.out.println("Scheduled Trains: " + scheduledTrains.size());
    }
}
