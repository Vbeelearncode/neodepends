package TTS;

import java.util.ArrayList;
import java.util.HashMap;

/**
 * NEW CLASS: Repository pattern
 * Manages TrainStation entities separately
 * Breaks up the god class from FIRST version
 */
public class TrainStationRepository {
    private HashMap<String, TrainStation> stations;

    public TrainStationRepository() {
        this.stations = new HashMap<>();
    }

    public void addStation(TrainStation station) {
        stations.put(station.getStationId(), station);
    }

    public TrainStation getStation(String stationId) {
        return stations.get(stationId);
    }

    public ArrayList<TrainStation> getAllStations() {
        return new ArrayList<>(stations.values());
    }

    public TrainStation findByName(String name) {
        for (TrainStation station : stations.values()) {
            if (station.getStationName().equalsIgnoreCase(name)) {
                return station;
            }
        }
        return null;
    }
}
