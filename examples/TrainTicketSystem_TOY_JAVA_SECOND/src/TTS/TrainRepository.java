package TTS;

import java.util.ArrayList;
import java.util.HashMap;

/**
 * NEW CLASS: Repository pattern
 * Manages Train entities separately
 */
public class TrainRepository {
    private HashMap<String, Train> trains;

    public TrainRepository() {
        this.trains = new HashMap<>();
    }

    public void addTrain(Train train) {
        trains.put(train.getTrainId(), train);
    }

    public Train getTrain(String trainId) {
        return trains.get(trainId);
    }

    public ArrayList<Train> getAllTrains() {
        return new ArrayList<>(trains.values());
    }

    public ArrayList<Train> getTrainsByRoute(String routeId) {
        ArrayList<Train> result = new ArrayList<>();
        for (Train train : trains.values()) {
            if (train.getRouteId().equals(routeId)) {
                result.add(train);
            }
        }
        return result;
    }
}
