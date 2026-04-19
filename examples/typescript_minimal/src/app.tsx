import { Circle } from "./circle";
import { area } from "./util";

export const App = () => {
    const c = new Circle(5);
    return <div>{area(c)}</div>;
};
