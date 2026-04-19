export function Route(path: string): ClassDecorator & MethodDecorator {
    return ((target: any, _propertyKey?: string | symbol) => {
        if (target && typeof target === "object") {
            (target as any).__route = path;
        }
        return target;
    }) as ClassDecorator & MethodDecorator;
}
