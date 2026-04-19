import { Shape } from "./shape";

export function area(s: Shape): number {
    return s.area();
}

export const perimeter = (s: Shape): number => {
    return s.perimeter();
};
