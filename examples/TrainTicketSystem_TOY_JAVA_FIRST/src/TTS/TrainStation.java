package TTS;

import java.util.ArrayList;

/**
 * Represents a train station
 */
public class TrainStation {
    private String stationCode;
    private String name;
    private String city;
    private ArrayList<TicketAgent> agents;
    private ArrayList<Train> availableTrains;

    public TrainStation(String stationCode, String name, String city) {
        this.stationCode = stationCode;
        this.name = name;
        this.city = city;
        this.agents = new ArrayList<>();
        this.availableTrains = new ArrayList<>();
    }

    public void addAgent(TicketAgent agent) {
        agents.add(agent);
        agent.setAssignedStation(this);
    }

    public void addTrain(Train train) {
        if (!availableTrains.contains(train)) {
            availableTrains.add(train);
        }
    }

    public void removeTrain(Train train) {
        availableTrains.remove(train);
    }

    public ArrayList<Train> searchTrains(TrainStation destination) {
        ArrayList<Train> matchingTrains = new ArrayList<>();
        for (Train train : availableTrains) {
            if (train.getRoute() != null &&
                train.getRoute().getDestination().equals(destination)) {
                matchingTrains.add(train);
            }
        }
        return matchingTrains;
    }

    public String getStationCode() {
        return stationCode;
    }

    public String getName() {
        return name;
    }

    public String getCity() {
        return city;
    }

    public ArrayList<TicketAgent> getAgents() {
        return agents;
    }

    public void displayInfo() {
        System.out.println("Station: " + name + " (" + stationCode + ")");
        System.out.println("City: " + city);
        System.out.println("Agents: " + agents.size());
        System.out.println("Available Trains: " + availableTrains.size());
    }

    @Override
    public boolean equals(Object obj) {
        if (this == obj) return true;
        if (obj == null || getClass() != obj.getClass()) return false;
        TrainStation that = (TrainStation) obj;
        return stationCode.equals(that.stationCode);
    }
}
