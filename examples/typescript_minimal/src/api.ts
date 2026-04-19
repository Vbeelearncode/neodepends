import { Route } from "./decorators";
import { Circle } from "./circle";

@Route("/users")
export class UsersController {
    private items: Circle[] = [];

    @Route("/list")
    list(): Circle[] {
        return this.items;
    }

    @Route("/add")
    add(radius: number): void {
        this.items.push(new Circle(radius));
    }
}
